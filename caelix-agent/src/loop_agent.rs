use std::{pin::Pin, sync::Arc};

use async_stream::stream;
use async_trait::async_trait;
use caelix_api::{
    Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig, LlmProvider, ToolCall,
};
use futures::{Stream, StreamExt};

use crate::tool_executor::execute_tools_static;
use super::util::{extract_pending_tool_calls, has_pending_tool_calls};

struct LoopAgent {
    def: Arc<AgentSpec>,
}

#[async_trait]
impl Agent for LoopAgent {
    async fn run(
        &self,
        mut messages: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: LlmConfig,
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

            let should_resume = has_pending_tool_calls(&messages);
        
            if should_resume {
                if let Some(tools) = extract_pending_tool_calls(&messages) {
                    match execute_tools_static(&def, &tools).await {
                        Ok(results) => {
                            for (id, name, res) in &results {
                                yield Ok(AgentOutputChunk::ToolResult {
                                    tool_name: name.clone(),
                                    result: res.clone(),
                                });
                                messages.push(ChatMessage::tool(id.clone(), res.clone()));
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
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
                            });

                            yield Ok(AgentOutputChunk::ToolCall {
                                tool_call_id: id,
                                name: name2,
                                arguments: arg2,
                            });
                        }

                        Ok(other) => yield Ok(other),
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }

                if final_tool_calls.is_empty() {
                    messages.push(ChatMessage::assistant(full_content));
                    break;
                } else {
                    messages.push(ChatMessage::assistant_tool_calls(full_content, final_tool_calls.clone()));
                }

                match execute_tools_static(&def, &final_tool_calls).await {
                    Ok(results) => {
                        for (id, name, res) in &results {
                            yield Ok(AgentOutputChunk::ToolResult {
                                tool_name: name.clone(),
                                result: res.clone(),
                            });
                            messages.push(ChatMessage::tool(id.clone(), res.clone()));
                        }
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }

            yield Ok(AgentOutputChunk::Finish { reason: "stop".into() });
        };

        Box::pin(stream)
    }
}

fn call_llm_static(
    def: &Arc<AgentSpec>,
    messages: &[ChatMessage],
    llm_provider: &Arc<dyn LlmProvider>,
    config: &LlmConfig,
) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>>
{
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

        while let Some(result) = stream.next().await {
            let chunk = match result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            if let Some(r) = &chunk.reasoning_content {
                if !r.is_empty() {
                    yield Ok(AgentOutputChunk::Reasoning {
                        content: r.clone(),
                    });
                }
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
    };
    Box::pin(s)
}

