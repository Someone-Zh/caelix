use crate::base::{AgentError, LlmConfig};
use crate::base::agent::traits::Agent;
use crate::base::agent::types::{AgentOutputChunk, AgentSpec};
use crate::base::provider::{ChatMessage, LlmProvider};
use crate::base::tool::{ToolCall};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt;

use super::converter::convert_chunk;
use super::tool_executor::execute_tool;

pub async fn run_agent_loop(
    agent: AgentSpec,
    messages: Vec<ChatMessage>,
    llm_provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
    let (tx, rx) = tokio::sync::mpsc::channel(128);
    let agent = Arc::new(agent);

    tokio::spawn(async move {
        let mut current_messages = messages;
        loop {
            let tool_defs = agent.get_tool_definitions();
            let mut stream = match llm_provider.chat_stream(&current_messages, &tool_defs, &config).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            let mut full_content = String::new();
            let mut tool_calls_buffer: Vec<(usize, String, String, String)> = Vec::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        // 安全处理思考内容
                        if let Some(r) = &chunk.reasoning_content {
                            if !r.is_empty() {
                                let _ = tx.send(Ok(AgentOutputChunk::Reasoning {
                                    content: r.clone(),
                                })).await;
                            }
                        }

                        // 安全处理文本内容
                        if let Some(c) = &chunk.content {
                            full_content.push_str(c);
                            let _ = tx.send(Ok(AgentOutputChunk::Content {
                                content: c.clone(),
                            })).await;
                        }

                        // 工具调用分片拼接
                        if let Some(tcs) = &chunk.tool_calls {
                            for tc in tcs {
                                let index = tc.index as usize;
                                let existing = tool_calls_buffer
                                    .iter_mut()
                                    .find(|(i, _, _, _)| *i == index);

                                if let Some((_, _, _, args)) = existing {
                                    args.push_str(tc.arguments.as_str().unwrap_or(""));
                                } else {
                                    let id = if tc.id.trim().is_empty() {
                                        format!("call_{index}")
                                    } else {
                                        tc.id.clone()
                                    };

                                    tool_calls_buffer.push((
                                        index,
                                        id,
                                        tc.name.clone(),
                                        tc.arguments.as_str().unwrap_or("").to_string(),
                                    ));
                                }
                            }
                        }

                        // 结束标志
                        if chunk.finish_reason.is_some() {
                            let _ = tx.send(convert_chunk(chunk)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }

            // 发送工具调用并构建 ToolCall 列表
            let mut final_tool_calls = Vec::new();
            for (_idx, id, name, args) in tool_calls_buffer.drain(..) {
                let clean_args = args.trim().to_string();
                
                // 拼接成 ToolCall 结构体（适配你的 ChatMessage::assistant_tool_calls）
                let tool_call = ToolCall {
                    id: id.clone(),
                    index: _idx as u32,
                    name: name.clone(),
                    arguments: serde_json::Value::String(clean_args.clone()),
                };
                final_tool_calls.push(tool_call);

                let _ = tx.send(Ok(AgentOutputChunk::ToolCall {
                    tool_call_id: id,
                    name,
                    arguments: clean_args,
                })).await;
            }
            // 无工具则退出
            if final_tool_calls.is_empty() {
                 let new_message = ChatMessage::assistant(
                    full_content,
                );
                current_messages.push(new_message);
                break;
            } else {
                let new_message = ChatMessage::assistant_tool_calls(
                    full_content,
                    final_tool_calls.clone(),
                );
                current_messages.push(new_message);
            }
            
            
            // 执行工具
            let mut tool_results = Vec::new();
            for tc in &final_tool_calls {
                match execute_tool(
                    &agent.tools,
                    tc,
                ).await {
                    Ok((name, result)) => {
                        tool_results.push((tc.id.clone(), name.clone(), result.clone()));
                        // 返回工具结果
                        let _ = tx.send(Ok(AgentOutputChunk::ToolResult {
                            tool_name: name,
                            result: result,
                        })).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }

            // 追加工具返回结果
            for (tc_id, _, result) in tool_results {
                current_messages.push(ChatMessage::tool(tc_id, result));
            }

        }

        let _ = tx.send(Ok(AgentOutputChunk::Finish { reason: "stop".into() })).await;
    });

    Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
}