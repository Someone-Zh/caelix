use std::{collections::HashMap, pin::Pin, sync::Arc};

use async_stream::stream;
use async_trait::async_trait;
use caelix_api::{
    Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig, LlmProvider, ToolCall,
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
    // 标准构造函数 ✅
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
    ) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>> {
        let def = self.def.clone();
        let llm = llm_provider.clone();
        let cfg = config.clone();

        let stream = stream! {
            yield Ok(AgentOutputChunk::Start {
                timestamp: chrono::Utc::now(),
            });
            messages = def.build_messages(messages);

            let should_resume = has_pending_tool_calls(&messages);
            let mut cumulative_usage = caelix_api::provider::TokenUsage::default();

            if should_resume
                && let Some(tools) = extract_pending_tool_calls(&messages) {
                    match execute_tools_static_with_pre_check(&def, &tools).await {
                        ToolExecutionBatchResult::Executed(results) => {
                            for (id, name, res) in &results {
                                yield Ok(AgentOutputChunk::ToolResult {
                                    tool_name: name.clone(),
                                    result: res.clone(),
                                });
                                messages.push(ChatMessage::tool(id.clone(), res.clone()));
                            }
                        }
                        ToolExecutionBatchResult::NeedApproval {
                            executed,
                            tool_call_id,
                            tool_name,
                            approval_type,
                            parameters,
                        } => {
                            // 先输出已执行的 tool_result 并追加到 messages
                            for (id, _name, res) in &executed {
                                yield Ok(AgentOutputChunk::ToolResult {
                                    tool_name: _name.clone(),
                                    result: res.clone(),
                                });
                                messages.push(ChatMessage::tool(id.clone(), res.clone()));
                            }
                            // 输出人工审批 chunk
                            yield Ok(AgentOutputChunk::ManualApproval {
                                tool_call_id: tool_call_id.clone(),
                                tool_name: tool_name.clone(),
                                approval_type: approval_type.clone(),
                                parameters: parameters.clone(),
                            });
                            // 中断：收尾 Finish（携带当前累计 usage）
                            let final_usage = if cumulative_usage.total_tokens > 0
                                || cumulative_usage.reasoning_tokens.is_some()
                                || cumulative_usage.cache_hit_tokens.is_some()
                            {
                                Some(cumulative_usage.clone())
                            } else {
                                None
                            };
                            yield Ok(AgentOutputChunk::Finish {
                                reason: "awaiting_approval".into(),
                                usage: final_usage,
                            });
                            return;
                        }
                    }
                }

            loop {
                if let Some(ctx) = RuntimeContext::try_current()
                    && ctx.cancellation_token().is_cancelled()
                {
                    yield Ok(AgentOutputChunk::Stopped {
                        reason: "cancelled_by_user".into(),
                    });
                    return;
                }

                let mut full_content = String::new();
                let mut final_tool_calls = Vec::new();

                {
                    let llm_stream = call_llm_static(&def, &messages, &llm, &cfg);

                    tokio::pin!(llm_stream);

                    // 在 LLM 流迭代期间监听取消信号：cancel future 只构造一次并 pin，
                    // select! 每轮通过 `&mut` 复用，避免每个 chunk 都重建 future 与 waiter。
                    let cancel_fut = async {
                        match RuntimeContext::try_current() {
                            Some(ctx) => ctx.cancellation_token().cancelled().await,
                            None => std::future::pending::<()>().await,
                        }
                    };
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
                                        let id = tool_call_id.clone();
                                        let name2 = name.clone();
                                        let arg2 = arguments.clone();
                                        let parsed_arguments = parse_tool_arguments(&arguments);

                                        final_tool_calls.push(ToolCall {
                                            id: tool_call_id,
                                            index: final_tool_calls.len() as u32,
                                            name,
                                            arguments: parsed_arguments,
                                            approval_state: None,
                                        });

                                        yield Ok(AgentOutputChunk::ToolCall {
                                            tool_call_id: id,
                                            name: name2,
                                            arguments: arg2,
                                        });
                                    }

                                    Ok(AgentOutputChunk::Finish { usage, .. }) => {
                                        // 拦截来自 call_llm_static 的内部 Finish，累加 usage（不会转发）
                                        if let Some(u) = usage {
                                            cumulative_usage.add(&u);
                                        }
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

                if final_tool_calls.is_empty() {
                    let new_msg = ChatMessage::assistant(full_content);
                    messages.push(new_msg.clone());
                    yield Ok(AgentOutputChunk::MessageUpdate { message: new_msg });
                    break;
                } else {
                    let new_msg =
                        ChatMessage::assistant_tool_calls(full_content, final_tool_calls.clone());
                    messages.push(new_msg.clone());
                    yield Ok(AgentOutputChunk::MessageUpdate { message: new_msg });
                }

                match execute_tools_static_with_pre_check(&def, &final_tool_calls).await {
                    ToolExecutionBatchResult::Executed(results) => {
                        for (id, name, res) in &results {
                            yield Ok(AgentOutputChunk::ToolResult {
                                tool_name: name.clone(),
                                result: res.clone(),
                            });
                            messages.push(ChatMessage::tool(id.clone(), res.clone()));
                        }
                    }
                    ToolExecutionBatchResult::NeedApproval {
                        executed,
                        tool_call_id,
                        tool_name,
                        approval_type,
                        parameters,
                    } => {
                        // 先写回已执行的 tool_result
                        for (id, _name, res) in &executed {
                            yield Ok(AgentOutputChunk::ToolResult {
                                tool_name: _name.clone(),
                                result: res.clone(),
                            });
                            messages.push(ChatMessage::tool(id.clone(), res.clone()));
                        }
                        // 输出人工审批 chunk
                        yield Ok(AgentOutputChunk::ManualApproval {
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            approval_type: approval_type.clone(),
                            parameters: parameters.clone(),
                        });
                        // 中断：收尾 Finish（携带当前累计 usage）
                        let final_usage = if cumulative_usage.total_tokens > 0
                            || cumulative_usage.reasoning_tokens.is_some()
                            || cumulative_usage.cache_hit_tokens.is_some()
                        {
                            Some(cumulative_usage.clone())
                        } else {
                            None
                        };
                        yield Ok(AgentOutputChunk::Finish {
                            reason: "awaiting_approval".into(),
                            usage: final_usage,
                        });
                        return;
                    }
                }
            }

            // 最终收尾 Finish：携带整个 Agent 会话的累计 usage
            let final_usage = if cumulative_usage.total_tokens > 0
                || cumulative_usage.reasoning_tokens.is_some()
                || cumulative_usage.cache_hit_tokens.is_some()
            {
                Some(cumulative_usage.clone())
            } else {
                None
            };
            yield Ok(AgentOutputChunk::Finish {
                reason: "stop".into(),
                usage: final_usage,
            });
        };

        Box::pin(stream)
    }

    fn get_spec(&self) -> Arc<AgentSpec> {
        self.def.clone()
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

    // 在 spawn 外部获取取消令牌，避免 task_local 跨 tokio::spawn 不传播的问题
    let cancel_token =
        RuntimeContext::try_current().map(|ctx| ctx.cancellation_token().child_token());

    let (tx, rx) = mpsc::channel(64);

    let handle = tokio::spawn(async move {
        let _ = tx.send(Ok(AgentOutputChunk::CallProvider {
            timestamp: chrono::Utc::now(),
            provider: llm.config().name.clone(),
            model: cfg.model_name.clone(),
        })).await;

        let mut stream = llm
            .chat_stream_with_cancel(&messages_clone, &tool_defs_clone, &cfg, cancel_token)
            .await;

        let mut tool_buffers: HashMap<usize, (String, String, String)> = HashMap::new();
        let mut cumulative_usage = caelix_api::provider::TokenUsage::default();

        while let Some(result) = stream.next().await {
            print!("[DEBUG][LoopAgent] stream result: {:#?}", result);

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

            if let Some(r) = &chunk.reasoning_content && !r.is_empty() {
                let _ = tx.send(Ok(AgentOutputChunk::Reasoning {
                    content: r.clone(),
                })).await;
            }

            if let Some(c) = &chunk.content {
                let _ = tx.send(Ok(AgentOutputChunk::Content {
                    content: c.clone(),
                })).await;
            }

            if let Some(tcs) = &chunk.tool_calls {
                for tc in tcs {
                    let idx = tc.index as usize;
                    let args_delta = tool_argument_delta(&tc.arguments);
                    if let Some((_, _, args)) = tool_buffers.get_mut(&idx) {
                        args.push_str(&args_delta);
                    } else {
                        let id = if tc.id.is_empty() {
                            format!("call_{idx}")
                        } else {
                            tc.id.clone()
                        };
                        tool_buffers.insert(
                            idx,
                            (id, tc.name.clone(), args_delta),
                        );
                    }
                }
            }
        }

        let mut ordered_tool_buffers = tool_buffers.into_iter().collect::<Vec<_>>();
        ordered_tool_buffers.sort_by_key(|(idx, _)| *idx);
        for (_idx, (id, name, args)) in ordered_tool_buffers {
            let _ = tx.send(Ok(AgentOutputChunk::ToolCall {
                tool_call_id: id,
                name,
                arguments: args.trim().to_string(),
            })).await;
        }

        if cumulative_usage.total_tokens > 0
            || cumulative_usage.reasoning_tokens.is_some()
            || cumulative_usage.cache_hit_tokens.is_some()
        {
            let _ = tx.send(Ok(AgentOutputChunk::Finish {
                reason: "llm_call_done".to_string(),
                usage: Some(cumulative_usage),
            })).await;
        }
    });

    let stream_with_abort = LlmStreamWithAbort { receiver: rx, handle };

    let s = stream! {
        tokio::pin!(stream_with_abort);

        let cancel_fut = async {
            match RuntimeContext::try_current() {
                Some(ctx) => ctx.cancellation_token().cancelled().await,
                None => std::future::pending::<()>().await,
            }
        };
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

fn tool_argument_delta(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_tool_arguments(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::Value::String(arguments.to_string()))
}
