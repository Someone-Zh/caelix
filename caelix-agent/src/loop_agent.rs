use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use caelix_api::{Agent, AgentError, AgentOutputChunk, AgentSpec, BaseContext, ChatMessage, LlmConfig, LlmProvider, MessageUpdateContext, PostToolExecContext, PreToolExecContext, RuntimeContext, ToolCall};
use futures::{Stream, StreamExt};

use crate::{converter::convert_chunk, tool_executor::execute_tool};

use super::util::{extract_pending_tool_calls,has_pending_tool_calls};

struct LoopAgent {
    def: Arc<AgentSpec>
}
#[async_trait]
impl Agent for  LoopAgent {
    async fn run(&self,
         mut messages: Vec<ChatMessage>,
         llm_provider: Arc<dyn LlmProvider>,
    config: LlmConfig) -> Result<Pin<Box<dyn Stream<Item = 
    Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        // 发送 Start 事件
        let _ = tx.send(Ok(AgentOutputChunk::Start { 
            timestamp: chrono::Utc::now() 
        })).await;
         // 检查是否需要从中断点恢复（最后一条消息有 tool calls）
        let should_resume_from_tools = has_pending_tool_calls(&messages);
        if !should_resume_from_tools {
            messages = self.def.build_messages(messages);
        }
        if should_resume_from_tools {
        // 从中断点恢复：直接执行工具
        if let Some(pending_tool_calls) = extract_pending_tool_calls(&messages) {
            // 执行工具
            match self.execute_tools(&pending_tool_calls, &tx).await {
                Ok(tool_results) => {
                    // 追加工具返回结果到消息列表
                    for (tc_id, _, result) in &tool_results {
                        messages.push(ChatMessage::tool(tc_id.clone(), result.clone()));
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        }
    }
    
    // 主循环
    loop {
        // 1. LLM 调用和结果收集
        let (full_content, final_tool_calls) = match self.call_llm(
            &messages,
            &llm_provider,
            &config,
            &tx,
        ).await {
            Ok(result) => result,
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)));
            }
        };
        
        // 2. 构建并添加 assistant 消息
        if final_tool_calls.is_empty() {
            // 无工具调用，添加普通 assistant 消息
            let new_message = ChatMessage::assistant(full_content);
            messages.push(new_message);
            
            break;
        } else {
            // 有工具调用，添加带 tool_calls 的 assistant 消息
            let new_message = ChatMessage::assistant_tool_calls(
                full_content,
                final_tool_calls.clone(),
            );
            messages.push(new_message);
            
        }
        
        // 3. 执行工具
        match self.execute_tools(&final_tool_calls, &tx).await {
            Ok(tool_results) => {
                // 追加工具返回结果
                for (tc_id, _, result) in tool_results {
                    messages.push(ChatMessage::tool(tc_id, result));
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)));
            }
        }
    }
    
    // 发送 Finish 事件
    let _ = tx.send(Ok(AgentOutputChunk::Finish { reason: "stop".into() })).await;
    Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

impl LoopAgent {
    async fn call_llm(&self, messages: &[ChatMessage],
        llm_provider: &Arc<dyn LlmProvider>,
        config: &LlmConfig,
        tx: &tokio::sync::mpsc::Sender<Result<AgentOutputChunk, AgentError>>,
    ) -> Result<(String, Vec<ToolCall>), AgentError> {
        // 1. 获取工具定义
        let tool_defs = self.def.get_tool_definitions();
        
        // 2. 发送 CallProvider 事件
        let provider_name = llm_provider.config().name.clone();
        let _ = tx.send(Ok(AgentOutputChunk::CallProvider {
            timestamp: chrono::Utc::now(),
            provider: provider_name,
            model: config.model_name.clone(),
        })).await;
        
        // 3. 调用 LLM
        let mut stream = llm_provider.chat_stream(messages, &tool_defs, config).await?;
        
        // 4. 收集响应
        let mut full_content = String::new();
        let mut tool_calls_buffer: Vec<(usize, String, String, String)> = Vec::new();
        
        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    // 处理 reasoning content
                    if let Some(r) = &chunk.reasoning_content
                        && !r.is_empty() {
                            let _ = tx.send(Ok(AgentOutputChunk::Reasoning {
                                content: r.clone(),
                            })).await;
                        }
                    
                    // 处理文本内容
                    if let Some(c) = &chunk.content {
                        full_content.push_str(c);
                        let _ = tx.send(Ok(AgentOutputChunk::Content {
                            content: c.clone(),
                        })).await;
                    }
                    
                    // 处理工具调用分片
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
                    
                    // 处理结束标志
                    if chunk.finish_reason.is_some() {
                        let _ = tx.send(convert_chunk(chunk)).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        
        // 5. 构建最终 ToolCall 列表并发送 ToolCall 事件
        let mut final_tool_calls = Vec::new();
        for (_idx, id, name, args) in tool_calls_buffer.drain(..) {
            let clean_args = args.trim().to_string();
            
            let tool_call = ToolCall {
                id: id.clone(),
                index: _idx as u32,
                name: name.clone(),
                arguments: serde_json::Value::String(clean_args.clone()),
            };
            final_tool_calls.push(tool_call);
            
            let _ = tx.send(Ok(AgentOutputChunk::ToolCall {
                tool_call_id: id.clone(),
                name: name.clone(),
                arguments: clean_args.clone(),
            })).await;
        }
        
        Ok((full_content, final_tool_calls))
    }

    async fn execute_tools(
        &self,
        tool_calls: &[ToolCall],
        tx: &tokio::sync::mpsc::Sender<Result<AgentOutputChunk, AgentError>>,
        ) -> Result<Vec<(String, String, String)>, AgentError> {
        let mut tool_results = Vec::new();
        
        for tc in tool_calls {
            // 执行工具
            match execute_tool(&self.def.tools, tc).await {
                Ok((name, tool_result)) => {
                    let result_str = match &tool_result.error {
                        Some(err) => format!("工具执行错误：{}", err),
                        None => tool_result.output.clone(),
                    };
                    tool_results.push((tc.id.clone(), name.clone(), result_str.clone()));
                    // 发送工具结果事件
                    let _ = tx.send(Ok(AgentOutputChunk::ToolResult {
                        tool_name: name.clone(),
                        result: result_str.clone(),
                    })).await;
                }
                Err(e) => return Err(e),
            }
        }
        
        Ok(tool_results)
    }
}
