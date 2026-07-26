use std::{pin::Pin, sync::Arc};

use async_stream::stream;
use async_trait::async_trait;
use caelix_api::{
    Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig, LlmProvider,
    ToolCall, ToolCallAggregator,
    context::RuntimeContext,
};
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;

use super::util::{extract_pending_tool_calls, has_pending_tool_calls};
use crate::tool_executor::{ToolExecutionBatchResult, execute_tools_static_with_pre_check};

pub struct LoopAgent {
    def: Arc<AgentSpec>,
}

impl LoopAgent {
    pub fn new(def: Arc<AgentSpec>) -> Self {
        Self { def }
    }
}

#[async_trait]
impl Agent for LoopAgent {
    async fn run(
        &self,
        mut messages: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: &LlmConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>>
    {
        let def = self.def.clone();
        let llm = llm_provider.clone();
        let cfg = config.clone();

        let stream = stream! {
            yield Ok(AgentOutputChunk::Start {
                timestamp: chrono::Utc::now(),
            });
            messages = def.build_messages(messages);

            let mut cumulative_usage = caelix_api::provider::TokenUsage::default();

            if has_pending_tool_calls(&messages)
                && let Some(tools) = extract_pending_tool_calls(&messages)
            {
                let (outcome, emitted) = apply_tool_execution_result(
                    execute_tools_static_with_pre_check(&def, &tools).await,
                    &mut messages,
                );
                for (name, res) in emitted {
                    yield Ok(AgentOutputChunk::ToolResult {
                        tool_name: name,
                        result: res,
                    });
                }
                match outcome {
                    ToolExecutionOutcome::Executed => {}
                    ToolExecutionOutcome::NeedApproval {
                        tool_call_id,
                        tool_name,
                        approval_type,
                        parameters,
                    } => {
                        yield Ok(AgentOutputChunk::ManualApproval {
                            tool_call_id,
                            tool_name,
                            approval_type,
                            parameters,
                        });
                        yield Ok(AgentOutputChunk::Finish {
                            reason: "awaiting_approval".into(),
                            usage: build_finish_usage(&cumulative_usage),
                        });
                        return;
                    }
                }
            }

            loop {
                if check_cancelled() {
                    yield Ok(AgentOutputChunk::Stopped {
                        reason: "cancelled_by_user".into(),
                    });
                    return;
                }

                let mut full_content = String::new();
                let mut tool_aggregator = ToolCallAggregator::new();

                {
                    let llm_stream = call_llm_static(&def, &messages, &llm, &cfg);
                    tokio::pin!(llm_stream);

                    let cancel_fut = cancel_future();
                    tokio::pin!(cancel_fut);

                    loop {
                        tokio::select! {
                            biased;
                            _ = &mut cancel_fut => {
                                yield Ok(AgentOutputChunk::Stopped {
                                    reason: "cancelled_by_user".into(),
                                });
                                return;
                            }
                            next = llm_stream.next() => {
                                let Some(item) = next else { break };
                                match item {
                                    Ok(AgentOutputChunk::Stopped { reason }) => {
                                        yield Ok(AgentOutputChunk::Stopped { reason });
                                        return;
                                    }
                                    Ok(AgentOutputChunk::Content { content }) => {
                                        full_content.push_str(&content);
                                        yield Ok(AgentOutputChunk::Content { content });
                                    }
                                    Ok(AgentOutputChunk::ToolCall { tool_call_id, name, arguments }) => {
                                        tool_aggregator.receive_delta(&ToolCall {
                                            id: tool_call_id.clone(),
                                            index: 0,
                                            name: name.clone(),
                                            arguments: serde_json::Value::String(arguments.clone()),
                                            approval_state: None,
                                        });
                                        yield Ok(AgentOutputChunk::ToolCall {
                                            tool_call_id,
                                            name,
                                            arguments,
                                        });
                                    }
                                    Ok(AgentOutputChunk::Finish { usage, .. }) => {
                                        if let Some(u) = usage {
                                            cumulative_usage.add(&u);
                                        }
                                        tool_aggregator.mark_done();
                                    }
                                    Ok(other) => yield Ok(other),
                                    Err(e) => {
                                        yield Err(e);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                let final_tool_calls = tool_aggregator.completed_tool_calls();

                if final_tool_calls.is_empty() {
                    let new_msg = ChatMessage::assistant(full_content);
                    messages.push(new_msg.clone());
                    yield Ok(AgentOutputChunk::MessageUpdate { message: new_msg });
                    break;
                } else {
                    let new_msg = ChatMessage::assistant_tool_calls(full_content, final_tool_calls.clone());
                    messages.push(new_msg.clone());
                    yield Ok(AgentOutputChunk::MessageUpdate { message: new_msg });
                }

                let (outcome, emitted) = apply_tool_execution_result(
                    execute_tools_static_with_pre_check(&def, &final_tool_calls).await,
                    &mut messages,
                );
                for (name, res) in emitted {
                    yield Ok(AgentOutputChunk::ToolResult {
                        tool_name: name,
                        result: res,
                    });
                }
                match outcome {
                    ToolExecutionOutcome::Executed => {}
                    ToolExecutionOutcome::NeedApproval {
                        tool_call_id,
                        tool_name,
                        approval_type,
                        parameters,
                    } => {
                        yield Ok(AgentOutputChunk::ManualApproval {
                            tool_call_id,
                            tool_name,
                            approval_type,
                            parameters,
                        });
                        yield Ok(AgentOutputChunk::Finish {
                            reason: "awaiting_approval".into(),
                            usage: build_finish_usage(&cumulative_usage),
                        });
                        return;
                    }
                }
            }

            yield Ok(AgentOutputChunk::Finish {
                reason: "stop".into(),
                usage: build_finish_usage(&cumulative_usage),
            });
        };

        Box::pin(stream)
    }

    fn get_spec(&self) -> Arc<AgentSpec> {
        self.def.clone()
    }
}

fn check_cancelled() -> bool {
    RuntimeContext::try_current()
        .is_some_and(|ctx| ctx.cancellation_token().is_cancelled())
}

fn cancel_future() -> impl std::future::Future<Output = ()> {
    async {
        match RuntimeContext::try_current() {
            Some(ctx) => ctx.cancellation_token().cancelled().await,
            None => std::future::pending::<()>().await,
        }
    }
}

fn build_finish_usage(
    cumulative_usage: &caelix_api::provider::TokenUsage,
) -> Option<caelix_api::provider::TokenUsage> {
    if cumulative_usage.total_tokens > 0
        || cumulative_usage.reasoning_tokens.is_some()
        || cumulative_usage.cache_hit_tokens.is_some()
    {
        Some(cumulative_usage.clone())
    } else {
        None
    }
}

enum ToolExecutionOutcome {
    Executed,
    NeedApproval {
        tool_call_id: String,
        tool_name: String,
        approval_type: caelix_api::tool::ToolApprovalType,
        parameters: serde_json::Value,
    },
}

fn apply_tool_execution_result(
    result: ToolExecutionBatchResult,
    messages: &mut Vec<ChatMessage>,
) -> (ToolExecutionOutcome, Vec<(String, String)>) {
    let mut emitted = Vec::new();
    match result {
        ToolExecutionBatchResult::Executed(results) => {
            for (id, name, res) in results {
                emitted.push((name, res.clone()));
                messages.push(ChatMessage::tool(id, res));
            }
            (ToolExecutionOutcome::Executed, emitted)
        }
        ToolExecutionBatchResult::NeedApproval {
            executed,
            tool_call_id,
            tool_name,
            approval_type,
            parameters,
        } => {
            for (id, name, res) in executed {
                emitted.push((name, res.clone()));
                messages.push(ChatMessage::tool(id, res));
            }
            (
                ToolExecutionOutcome::NeedApproval {
                    tool_call_id,
                    tool_name,
                    approval_type,
                    parameters,
                },
                emitted,
            )
        }
    }
}

struct LlmStreamWithAbort {
    receiver: mpsc::Receiver<Result<AgentOutputChunk, AgentError>>,
    handle: tokio::task::JoinHandle<()>,
}

impl LlmStreamWithAbort {
    fn abort(&self) {
        self.handle.abort();
    }
}

impl Drop for LlmStreamWithAbort {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl Stream for LlmStreamWithAbort {
    type Item = Result<AgentOutputChunk, AgentError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

fn call_llm_static<'a>(
    def: &'a Arc<AgentSpec>,
    messages: &'a [ChatMessage],
    llm_provider: &'a Arc<dyn LlmProvider>,
    config: &'a LlmConfig,
) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'a>> {
    let tool_defs = def.get_tool_definitions();
    let llm = llm_provider.clone();
    let cfg = config.clone();
    let messages_clone = messages.to_vec();
    let tool_defs_clone = tool_defs.to_vec();

    let cancel_token =
        RuntimeContext::try_current().map(|ctx| ctx.cancellation_token().child_token());

    let (tx, rx) = mpsc::channel(64);

    let handle = tokio::spawn(async move {
        let _ = tx
            .send(Ok(AgentOutputChunk::CallProvider {
                timestamp: chrono::Utc::now(),
                provider: llm.config().name.clone(),
                model: cfg.model_name.clone(),
            }))
            .await;

        let mut stream = llm
            .chat_stream_with_cancel(&messages_clone, &tool_defs_clone, &cfg, cancel_token)
            .await;

        let mut cumulative_usage = caelix_api::provider::TokenUsage::default();

        while let Some(result) = stream.next().await {
            let chunk = match result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            if let Some(u) = &chunk.usage {
                cumulative_usage.add(u);
            }

            if let Some(r) = &chunk.reasoning_content
                && !r.is_empty()
            {
                let _ = tx
                    .send(Ok(AgentOutputChunk::Reasoning {
                        content: r.clone(),
                    }))
                    .await;
            }

            if let Some(c) = &chunk.content {
                let _ = tx
                    .send(Ok(AgentOutputChunk::Content {
                        content: c.clone(),
                    }))
                    .await;
            }

            if let Some(tcs) = &chunk.tool_calls {
                for tc in tcs {
                    let args = match &tc.arguments {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let _ = tx
                        .send(Ok(AgentOutputChunk::ToolCall {
                            tool_call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments: args,
                        }))
                        .await;
                }
            }
        }

        if cumulative_usage.total_tokens > 0
            || cumulative_usage.reasoning_tokens.is_some()
            || cumulative_usage.cache_hit_tokens.is_some()
        {
            let _ = tx
                .send(Ok(AgentOutputChunk::Finish {
                    reason: "llm_call_done".to_string(),
                    usage: Some(cumulative_usage),
                }))
                .await;
        }
    });

    let stream_with_abort = LlmStreamWithAbort {
        receiver: rx,
        handle,
    };

    let s = stream! {
        tokio::pin!(stream_with_abort);

        let cancel_fut = cancel_future();
        tokio::pin!(cancel_fut);

        loop {
            tokio::select! {
                biased;
                _ = &mut cancel_fut => {
                    stream_with_abort.abort();
                    yield Ok(AgentOutputChunk::Stopped {
                        reason: "cancelled_by_user".into(),
                    });
                    return;
                }
                next = stream_with_abort.next() => {
                    match next {
                        Some(item) => yield item,
                        None => break,
                    }
                }
            }
        }
    };

    Box::pin(s)
}
