use caelix_api::agent::{AgentOutputChunk, AgentSpec};
use caelix_api::provider::{ChatMessage, LlmProvider};
use caelix_api::provider::LlmConfig;
use caelix_message::agent_message::{AgentMessage, AgentMessageType};
use caelix_runtime::context::RuntimeContext;
use futures::StreamExt;
use std::sync::Arc;

/// 执行 agent 并将流发送到消息总线的公共函数
/// 
/// # 参数
/// * `agent_spec` - Agent 规格
/// * `messages` - 聊天消息列表
/// * `provider` - LLM Provider
/// * `config` - LLM 配置
/// * `session_id` - 会话 ID
/// * `request_id` - 请求 ID
/// * `span_id` - Span ID
/// * `agent_name` - Agent 名称（可选）
/// 
/// # 返回
/// 返回收集到的纯文本内容（仅 Content 类型）
#[allow(clippy::too_many_arguments)]
pub async fn execute_agent_with_messaging(
    agent_spec: Arc<AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn LlmProvider>,
    config: &LlmConfig,
    session_id: String,
    request_id: String,
    span_id: String,
    agent_name: Option<String>,
) -> Result<String, anyhow::Error> {
    // 获取当前的 RuntimeContext
    let runtime_ctx = RuntimeContext::current();
    
    // 从 ContextProvider 获取 MessageSender
    let message_sender = runtime_ctx.get_message_sender()
        .ok_or_else(|| anyhow::anyhow!("MessageSender not available in RuntimeContext"))?;
    
    // 使用 loop_runner 执行 agent
    let stream = super::loop_runner::run_agent_loop(
        (*agent_spec).clone(),
        messages,
        provider,
        config.clone(),
    ).await?;
    
    let mut result_content = String::new();
    let mut stream = stream;
    
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // 提取内容并积累
                let content = extract_chunk_content(&chunk);
                if !content.is_empty() {
                    result_content.push_str(&content);
                }
                
                // 发送 Chunk 到消息总线
                let chunk_msg = AgentMessage {
                    session_id: session_id.clone(),
                    request_id: request_id.clone(),
                    span_id: span_id.clone(),
                    r#type: AgentMessageType::Chunk,
                    timestamp: chrono::Utc::now(),
                    content: content.clone(),
                    agent_name: agent_name.clone(),
                };
                let _ = message_sender.send_agent(chunk_msg);
                
                // 如果是 Finish，发送 ChunkEnd
                if matches!(chunk, AgentOutputChunk::Finish { .. }) {
                    let end_msg = AgentMessage {
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        span_id: span_id.clone(),
                        r#type: AgentMessageType::ChunkEnd,
                        timestamp: chrono::Utc::now(),
                        content: String::new(),
                        agent_name: agent_name.clone(),
                    };
                    let _ = message_sender.send_agent(end_msg);
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Agent execution error: {:?}", e));
            }
        }
    }
    
    Ok(result_content)
}

/// 从 AgentOutputChunk 提取文本内容
fn extract_chunk_content(chunk: &AgentOutputChunk) -> String {
    match chunk {
        AgentOutputChunk::Content { content } => content.clone(),
        // ✅ 移除 [思考] 标签,直接返回 reasoning 内容,让 CLI 端统一处理显示
        AgentOutputChunk::Reasoning { content } => content.clone(),
        AgentOutputChunk::ToolCall { name, arguments, .. } => {
            format!("\n[工具调用] {}({})", name, arguments)
        }
        AgentOutputChunk::ToolResult { tool_name, result } => {
            format!("\n[工具结果] {}: {}", tool_name, result)
        }
        AgentOutputChunk::Start { timestamp } => {
            format!("\n[开始] {}", timestamp.format("%H:%M:%S"))
        },
        AgentOutputChunk::CallProvider { timestamp, provider, model } => {
            format!("\n[调用模型] {} {}@{}", timestamp.format("%H:%M:%S"), provider, model)
        },
        AgentOutputChunk::Finish { .. } => String::new(),
    }
}
