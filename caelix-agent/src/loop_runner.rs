use caelix_api::error::AgentError;
use caelix_api::provider::LlmConfig;
use caelix_api::agent::{AgentOutputChunk, AgentSpec};
use caelix_api::provider::{ChatMessage, LlmProvider};
use caelix_api::tool::ToolCall;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt;

use super::converter::convert_chunk;
use super::tool_executor::execute_tool;
use caelix_runtime::context::RuntimeContext;
use caelix_runtime::hooks::{BaseContext, PreToolExecContext, PostToolExecContext, MessageUpdateContext};

/// 检查消息列表的最后一条消息是否包含未执行的 tool calls
fn has_pending_tool_calls(messages: &[ChatMessage]) -> bool {
    messages.last().map_or(false, |msg| {
        msg.tool_calls.is_some() && !msg.tool_calls.as_ref().unwrap().is_empty()
    })
}

/// 从最后一条消息中提取待执行的 tool calls
fn extract_pending_tool_calls(messages: &[ChatMessage]) -> Option<Vec<ToolCall>> {
    messages.last().and_then(|msg| {
        msg.tool_calls.clone().filter(|calls| !calls.is_empty())
    })
}

pub async fn run_agent_loop(
    agent: AgentSpec,
    messages: Vec<ChatMessage>,
    llm_provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
    let (tx, rx) = tokio::sync::mpsc::channel(128);
    let agent = Arc::new(agent);

    // 获取当前的 RuntimeContext（如果存在）
    let runtime_ctx = std::panic::catch_unwind(|| {
        caelix_runtime::context::RuntimeContext::current()
    }).ok();

    tokio::spawn(async move {
        // 如果有 RuntimeContext，在 scope 中执行；否则返回错误
        if let Some(ctx) = runtime_ctx {
            caelix_runtime::context::RuntimeContext::scope(ctx, async move {
                run_agent_loop_inner(agent, messages, llm_provider, config, tx).await;
            }).await;
        } else {
            // 没有 RuntimeContext 时，发送错误并退出
            let _ = tx.send(Err(AgentError::TaskError(
                "No RuntimeContext available. This should not happen in normal operation.".to_string()
            ))).await;
        }
    });

    Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
}

async fn run_agent_loop_inner(
    agent: Arc<AgentSpec>,
    mut current_messages: Vec<ChatMessage>,
    llm_provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
    tx: tokio::sync::mpsc::Sender<Result<AgentOutputChunk, AgentError>>,
) {
    // 发送 Start 事件
    let _ = tx.send(Ok(AgentOutputChunk::Start { 
        timestamp: chrono::Utc::now() 
    })).await;
    
    // 检查是否需要从中断点恢复（最后一条消息有 tool calls）
    let should_resume_from_tools = has_pending_tool_calls(&current_messages);
    
    if should_resume_from_tools {
        // 从中断点恢复：直接执行工具
        if let Some(pending_tool_calls) = extract_pending_tool_calls(&current_messages) {
            // 执行工具
            match execute_tools_and_collect_results(&agent, &pending_tool_calls, &tx).await {
                Ok(tool_results) => {
                    // 追加工具返回结果到消息列表
                    for (tc_id, _, result) in &tool_results {
                        current_messages.push(ChatMessage::tool(tc_id.clone(), result.clone()));
                    }
                    
                    // 调用消息更新钩子
                    call_message_update_hook(&agent, &current_messages).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }
    }
    
    // 主循环
    loop {
        // 1. LLM 调用和结果收集
        let (full_content, final_tool_calls) = match call_llm_and_collect(
            &agent,
            &current_messages,
            &llm_provider,
            &config,
            &tx,
        ).await {
            Ok(result) => result,
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return;
            }
        };
        
        // 2. 构建并添加 assistant 消息
        if final_tool_calls.is_empty() {
            // 无工具调用，添加普通 assistant 消息
            let new_message = ChatMessage::assistant(full_content);
            current_messages.push(new_message);
            
            // 调用消息更新钩子
            call_message_update_hook(&agent, &current_messages).await;
            
            // 退出循环
            break;
        } else {
            // 有工具调用，添加带 tool_calls 的 assistant 消息
            let new_message = ChatMessage::assistant_tool_calls(
                full_content,
                final_tool_calls.clone(),
            );
            current_messages.push(new_message);
            
            // 调用消息更新钩子
            call_message_update_hook(&agent, &current_messages).await;
        }
        
        // 3. 执行工具
        match execute_tools_and_collect_results(&agent, &final_tool_calls, &tx).await {
            Ok(tool_results) => {
                // 追加工具返回结果
                for (tc_id, _, result) in tool_results {
                    current_messages.push(ChatMessage::tool(tc_id, result));
                }
                
                // 调用消息更新钩子（工具结果添加后）
                call_message_update_hook(&agent, &current_messages).await;
            }
            Err(e) => {
                let _ = tx.send(Err(e)).await;
                return;
            }
        }
    }
    
    // 发送 Finish 事件
    let _ = tx.send(Ok(AgentOutputChunk::Finish { reason: "stop".into() })).await;
}

/// 调用 LLM 并收集结果
async fn call_llm_and_collect(
    agent: &Arc<AgentSpec>,
    current_messages: &[ChatMessage],
    llm_provider: &Arc<dyn LlmProvider>,
    config: &LlmConfig,
    tx: &tokio::sync::mpsc::Sender<Result<AgentOutputChunk, AgentError>>,
) -> Result<(String, Vec<ToolCall>), AgentError> {
    // 1. 获取工具定义
    let tool_defs = agent.get_tool_definitions();
    
    // 2. 发送 CallProvider 事件
    let provider_name = llm_provider.config().name.clone();
    let _ = tx.send(Ok(AgentOutputChunk::CallProvider {
        timestamp: chrono::Utc::now(),
        provider: provider_name,
        model: config.model_name.clone(),
    })).await;
    
    // 3. 调用 LLM
    let mut stream = llm_provider.chat_stream(current_messages, &tool_defs, config).await?;
    
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

/// 执行工具并收集结果
async fn execute_tools_and_collect_results(
    agent: &Arc<AgentSpec>,
    tool_calls: &[ToolCall],
    tx: &tokio::sync::mpsc::Sender<Result<AgentOutputChunk, AgentError>>,
) -> Result<Vec<(String, String, String)>, AgentError> {
    let mut tool_results = Vec::new();
    
    for tc in tool_calls {
        // 执行工具前钩子
        if let Ok(runtime_ctx) = std::panic::catch_unwind(RuntimeContext::current) {
            let base_ctx = BaseContext {
                session_id: runtime_ctx.get_session_id().to_string(),
                request_id: runtime_ctx.get_request_id().to_string(),
                span_id: runtime_ctx.get_span_id().to_string(),
                agent_name: agent.name.clone(),
                agent_group: agent.group.as_ref().map(|g| g.to_string()),
            };
            
            let mut tool_ctx = PreToolExecContext {
                base: base_ctx,
                tool_name: tc.name.clone(),
                tool_args: tc.arguments.clone(),
            };
            
            if let Some(hook_executor) = runtime_ctx.get_hook_executor() {
                if let Err(e) = hook_executor.execute_pre_tool_exec(&mut tool_ctx).await {
                    eprintln!("Warning: Pre-tool-exec hook failed: {}", e);
                }
            } else {
                eprintln!("Warning: HookExecutor not available, skipping pre-tool-exec hook");
            }
        }
        
        // 执行工具
        match execute_tool(&agent.tools, tc).await {
            Ok((name, tool_result)) => {
                // 执行工具后钩子
                let mut final_result = tool_result.clone();
                
                if let Ok(runtime_ctx) = std::panic::catch_unwind(RuntimeContext::current) {
                    let base_ctx = BaseContext {
                        session_id: runtime_ctx.get_session_id().to_string(),
                        request_id: runtime_ctx.get_request_id().to_string(),
                        span_id: runtime_ctx.get_span_id().to_string(),
                        agent_name: agent.name.clone(),
                        agent_group: agent.group.as_ref().map(|g| g.to_string()),
                    };
                    
                    let mut post_tool_ctx = PostToolExecContext {
                        base: base_ctx,
                        tool_name: name.clone(),
                        tool_args: tc.arguments.clone(),
                        tool_result: final_result.clone(),
                    };
                    
                    if let Some(hook_executor) = runtime_ctx.get_hook_executor() {
                        if let Err(e) = hook_executor.execute_post_tool_exec(&mut post_tool_ctx).await {
                            eprintln!("Warning: Post-tool-exec hook failed: {}", e);
                        }
                    } else {
                        eprintln!("Warning: HookExecutor not available, skipping post-tool-exec hook");
                    }
                    
                    final_result = post_tool_ctx.tool_result;
                }
                
                // 转换为字符串
                let result_str = match &final_result.error {
                    Some(err) => format!("工具执行错误：{}", err),
                    None => final_result.output.clone(),
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

/// 调用消息更新钩子
async fn call_message_update_hook(
    agent: &Arc<AgentSpec>,
    current_messages: &[ChatMessage],
) {
    if let Ok(runtime_ctx) = std::panic::catch_unwind(RuntimeContext::current) {
        let base_ctx = BaseContext {
            session_id: runtime_ctx.get_session_id().to_string(),
            request_id: runtime_ctx.get_request_id().to_string(),
            span_id: runtime_ctx.get_span_id().to_string(),
            agent_name: agent.name.clone(),
            agent_group: agent.group.as_ref().map(|g| g.to_string()),
        };
        
        let msg_ctx = MessageUpdateContext {
            base: base_ctx,
            messages: Arc::new(current_messages.to_vec()),
        };
        
        if let Some(hook_executor) = runtime_ctx.get_hook_executor() {
            if let Err(e) = hook_executor.execute_message_update(&msg_ctx).await {
                eprintln!("Warning: Message update hook failed: {}", e);
            }
        } else {
            eprintln!("Warning: HookExecutor not available, skipping message update hook");
        }
    }
}