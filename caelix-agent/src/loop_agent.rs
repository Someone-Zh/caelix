use std::{pin::Pin, sync::Arc};

use async_stream::stream;
use async_trait::async_trait;
use caelix_api::{
    Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig, LlmProvider, ToolCall,
};
use futures::{Stream, StreamExt};

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
                let llm_stream = call_llm_static(&def, &messages, &llm, &cfg);

                tokio::pin!(llm_stream);
                let mut full_content = String::new();
                let mut final_tool_calls = Vec::new();

                while let Some(item) = llm_stream.next().await {
                    match item {
                        Ok(AgentOutputChunk::Content { content }) => {
                            full_content.push_str(&content);
                            yield Ok(AgentOutputChunk::Content { content });
                        }

                        Ok(AgentOutputChunk::ToolCall { tool_call_id, name, arguments }) => {
                            let id = tool_call_id.clone();
                            let name2 = name.clone();
                            let arg2 = arguments.clone();

                            final_tool_calls.push(ToolCall {
                                id: tool_call_id,
                                index: final_tool_calls.len() as u32,
                                name,
                                arguments: serde_json::Value::String(arguments),
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

fn call_llm_static(
    def: &Arc<AgentSpec>,
    messages: &[ChatMessage],
    llm_provider: &Arc<dyn LlmProvider>,
    config: &LlmConfig,
) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>> {
    let tool_defs = def.get_tool_definitions();
    let llm = llm_provider.clone();
    let msgs = messages.to_vec();
    let cfg = config.clone();

    let s = stream! {
        yield Ok(AgentOutputChunk::CallProvider {
            timestamp: chrono::Utc::now(),
            provider: llm.config().name.clone(),
            model: cfg.model_name.clone(),
        });

        let mut stream = llm.chat_stream(&msgs, &tool_defs, &cfg).await;

        let mut tool_buffers: Vec<(usize, String, String, String)> = Vec::new();
        let mut cumulative_usage = caelix_api::provider::TokenUsage::default();

        while let Some(result) = stream.next().await {
            let chunk = match result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            if let Some(u) = &chunk.usage {
                cumulative_usage.add(u);
            }

            if let Some(r) = &chunk.reasoning_content
                && !r.is_empty()
            {
                yield Ok(AgentOutputChunk::Reasoning {
                    content: r.clone(),
                });
            }

            if let Some(c) = &chunk.content {
                yield Ok(AgentOutputChunk::Content {
                    content: c.clone(),
                });
            }

            if let Some(tcs) = &chunk.tool_calls {
                for tc in tcs {
                    let idx = tc.index as usize;
                    if let Some((_, _, _, args)) = tool_buffers.iter_mut().find(|(i, _, _, _)| *i == idx) {
                        args.push_str(tc.arguments.as_str().unwrap_or(""));
                    } else {
                        let id = if tc.id.is_empty() {
                            format!("call_{idx}")
                        } else {
                            tc.id.clone()
                        };
                        tool_buffers.push((
                            idx,
                            id,
                            tc.name.clone(),
                            tc.arguments.as_str().unwrap_or("").to_string(),
                        ));
                    }
                }
            }
        }

        for (_idx, id, name, args) in tool_buffers {
            yield Ok(AgentOutputChunk::ToolCall {
                tool_call_id: id,
                name,
                arguments: args.trim().to_string(),
            });
        }

        // 单次 LLM 调用结束：产出 Finish 并携带本次的累计 usage
        // （外层 LoopAgent 会拦截这种 Finish，累加并最终产出全局 Finish）
        if cumulative_usage.total_tokens > 0
            || cumulative_usage.reasoning_tokens.is_some()
            || cumulative_usage.cache_hit_tokens.is_some()
        {
            yield Ok(AgentOutputChunk::Finish {
                reason: "llm_call_done".to_string(),
                usage: Some(cumulative_usage),
            });
        }
    };
    Box::pin(s)
}
