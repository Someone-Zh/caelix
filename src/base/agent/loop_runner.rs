use crate::base::{AgentError, LlmConfig};
use crate::base::agent::traits::Agent;
use crate::base::agent::types::{AgentSpec, AgentOutputChunk};
use crate::base::provider::{ChatMessage, LlmProvider};
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
                        // 实时输出思考内容
                        if let Some(r) = &chunk.reasoning_content {
                            let _ = tx.send(Ok(AgentOutputChunk::Reasoning {
                                content: r.clone(),
                            })).await;
                        }

                        // 实时输出文本内容
                        if let Some(c) = &chunk.content {
                            full_content.push_str(c);
                            let _ = tx.send(Ok(AgentOutputChunk::Content {
                                content: c.clone(),
                            })).await;
                        }

                        // 工具调用拼接：完全适配 Qwen 格式（index 匹配 + 空 ID 兼容）
                        if let Some(tcs) = &chunk.tool_calls {
                            for tc in tcs {
                                let index = tc.index as usize;

                                // 按 index 匹配工具（Qwen 核心规则）
                                let existing = tool_calls_buffer
                                    .iter_mut()
                                    .find(|(i, _, _, _)| *i == index);

                                if let Some((_, _, _, args)) = existing {
                                    // 后续 chunk：只拼参数
                                    args.push_str(tc.arguments.as_str().unwrap_or(""));
                                } else {
                                    // 第一个 chunk：保存 id + name
                                    let id = if tc.id.is_empty() {
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

            // 发送所有工具
            let mut final_tool_calls = Vec::new();
            for (_idx, id, name, args) in tool_calls_buffer.drain(..) {
                let clean_args = args.trim().to_string();
                final_tool_calls.push((id.clone(), name.clone(), clean_args.clone()));

                let _ = tx.send(Ok(AgentOutputChunk::ToolCall {
                    tool_call_id: id,
                    name,
                    arguments: clean_args,
                })).await;
            }

            // 无工具则结束
            if final_tool_calls.is_empty() {
                break;
            }

            // 批量执行工具（唯一阻塞点）
            let mut tool_results = Vec::new();
            for (tc_id, tc_name, tc_args) in final_tool_calls {
                match execute_tool(&agent.tools, tc_name, tc_args, &tx).await {
                    Ok((name, result)) => {
                        tool_results.push((tc_id, name, result));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }

            // 更新上下文
            current_messages.push(ChatMessage::assistant(full_content));
            for (tc_id, _, result) in tool_results {
                current_messages.push(ChatMessage::tool(tc_id, result));
            }
        }

        let _ = tx.send(Ok(AgentOutputChunk::Finish { reason: "stop".into() })).await;
    });

    Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
}