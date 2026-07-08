use std::sync::Arc;

use caelix_api::{
    Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig,
    message::{AgentMessage, AgentMessageType},
};
use futures::StreamExt;
use tracing::Instrument;

use crate::loop_agent::LoopAgent;

/// 执行 Agent，将各类输出分片发送到消息总线，并返回累积的文本内容
///
/// 会从 `RuntimeContext`（task_local）获取 session/request/span/trace ID，
/// 从 `ContextProvider`（全局）获取 `message_bus`，然后按以下规则转发：
///
/// - `Content` → `AgentMessageType::Chunk`（流式内容，供前端实时显示）
/// - `MessageUpdate` → `AgentMessageType::Msg`（完整 ChatMessage，持久化到历史）
/// - `Start | CallProvider | Reasoning | ToolCall | ToolResult | Finish`
///   → `AgentMessageType::Event`（触发事件标记，供前端按历史还原时机）
///
/// 当遇到 `Finish` 分片时额外发送一条 `ChunkEnd` 消息，
/// 用于通知消息总线消费者清空该请求的缓冲。
pub async fn run_agent(
    agent_spec: Arc<AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
) -> Result<String, AgentError> {
    let agent_name = Some(agent_spec.name.clone());
    let agent = LoopAgent::new(agent_spec);
    let provider_name = provider.config().name.clone();
    let model_name = config.model_name.clone();
    let mut stream = agent.run(messages, provider, config).await;

    // 从 RuntimeContext 中获取 tracing ID；若不存在则回退为空字符串。
    let (session_id, request_id, span_id, trace_id, session_span) =
        match caelix_api::context::RuntimeContext::try_current() {
            Some(ctx) => (
                ctx.get_session_id().to_string(),
                ctx.get_request_id().to_string(),
                ctx.get_span_id().to_string(),
                ctx.get_trace_id().to_string(),
                Some(ctx.session_span()),
            ),
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                None,
            ),
        };

    let fut = async move {
        tracing::info!(
            agent = agent_name.as_deref().unwrap_or(""),
            "run_agent start"
        );

        // 从 ContextProvider 中获取 message_bus（可选；无则静默跳过）
        let maybe_bus = caelix_api::context::try_caelix_context();

        let mut result_content = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            // 根据分片类型构造并发送消息到 message_bus
            if let Some(bus_ctx) = &maybe_bus {
                let msg_type = match &chunk {
                    AgentOutputChunk::Content { .. } => Some((AgentMessageType::Chunk, None)),
                    AgentOutputChunk::MessageUpdate { message } => {
                        let payload = match serde_json::to_string(message) {
                            Ok(json) => json,
                            Err(_) => message.content.clone(),
                        };
                        Some((AgentMessageType::Msg, Some(payload)))
                    }
                    AgentOutputChunk::Start { timestamp } => Some((
                        AgentMessageType::Event,
                        Some(format!("[开始] {}", timestamp.format("%H:%M:%S"))),
                    )),
                    AgentOutputChunk::CallProvider {
                        timestamp,
                        provider,
                        model,
                    } => Some((
                        AgentMessageType::Event,
                        Some(format!(
                            "[调用模型] {} {}@{}",
                            timestamp.format("%H:%M:%S"),
                            provider,
                            model
                        )),
                    )),
                    AgentOutputChunk::Reasoning { content } => {
                        Some((AgentMessageType::Event, Some(format!("[思考] {}", content))))
                    }
                    AgentOutputChunk::ToolCall {
                        tool_call_id,
                        name,
                        arguments,
                    } => Some((
                        AgentMessageType::Event,
                        Some(format!(
                            "[工具调用] {}({}): {}",
                            tool_call_id, name, arguments
                        )),
                    )),
                    AgentOutputChunk::ToolResult { tool_name, result } => Some((
                        AgentMessageType::Event,
                        Some(format!("[工具结果] {}: {}", tool_name, result)),
                    )),
                    AgentOutputChunk::ManualApproval {
                        tool_call_id,
                        tool_name,
                        approval_type,
                        parameters,
                    } => {
                        let v = serde_json::json!({
                            "tool_call_id": tool_call_id,
                            "tool_name": tool_name,
                            "approval_type": format!("{:?}", approval_type),
                            "parameters": parameters,
                        });
                        let content = serde_json::to_string(&v).unwrap_or_else(|_| {
                            format!(
                                "[需要审批] tool_call_id={}, tool_name={}",
                                tool_call_id, tool_name
                            )
                        });
                        Some((AgentMessageType::ManualApproval, Some(content)))
                    }
                    AgentOutputChunk::Finish { usage, .. } => {
                        // 若本次请求有 usage 信息，则写入用量追踪器
                        if let Some(u) = usage.clone()
                            && let Some(bus_ctx) = &maybe_bus
                            && let Some(tracker) = bus_ctx.usage_tracker()
                        {
                            let _ = tracker
                                .accumulate(caelix_api::provider::UsageRecord {
                                    session_id: session_id.clone(),
                                    request_id: request_id.clone(),
                                    trace_id: trace_id.clone(),
                                    provider: provider_name.clone(),
                                    model: model_name.clone(),
                                    agent: agent_name.clone(),
                                    prompt_tokens: u.prompt_tokens,
                                    completion_tokens: u.completion_tokens,
                                    total_tokens: u.total_tokens,
                                    reasoning_tokens: u.reasoning_tokens,
                                    cache_hit_tokens: u.cache_hit_tokens,
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                })
                                .await;
                        }
                        Some((AgentMessageType::Event, None))
                    }
                    AgentOutputChunk::Stopped { reason } => Some((
                        AgentMessageType::Event,
                        Some(format!("[已停止] {}", reason)),
                    )),
                };

                if let Some((msg_type, payload)) = msg_type {
                    let content = match (&msg_type, payload) {
                        (AgentMessageType::Msg, Some(payload)) => payload,
                        (AgentMessageType::Chunk, _) => extract_chunk_text(&chunk),
                        (AgentMessageType::Event, Some(desc)) => desc,
                        (AgentMessageType::Event, None) => String::new(),
                        _ => String::new(),
                    };

                    let agent_msg = AgentMessage {
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        span_id: span_id.clone(),
                        trace_id: trace_id.clone(),
                        r#type: msg_type,
                        timestamp: chrono::Utc::now(),
                        content,
                        agent_name: agent_name.clone(),
                        usage: None,
                    };
                    let _ = bus_ctx.message_bus().send_agent(agent_msg);
                }

                // 遇到 Finish 时追加发送 ChunkEnd，清空消费者的请求缓冲
                if let AgentOutputChunk::Finish { usage, .. } = &chunk {
                    let end_msg = AgentMessage {
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        span_id: span_id.clone(),
                        trace_id: trace_id.clone(),
                        r#type: AgentMessageType::ChunkEnd,
                        timestamp: chrono::Utc::now(),
                        content: String::new(),
                        agent_name: agent_name.clone(),
                        usage: usage.clone(),
                    };
                    let _ = bus_ctx.message_bus().send_agent(end_msg);
                }

                // 遇到 Stopped 时也发送 ChunkEnd 并中断
                if let AgentOutputChunk::Stopped { .. } = &chunk {
                    let end_msg = AgentMessage {
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        span_id: span_id.clone(),
                        trace_id: trace_id.clone(),
                        r#type: AgentMessageType::ChunkEnd,
                        timestamp: chrono::Utc::now(),
                        content: String::new(),
                        agent_name: agent_name.clone(),
                        usage: None,
                    };
                    let _ = bus_ctx.message_bus().send_agent(end_msg);
                }
            }

            // 累积最终文本内容（仅 Content 类型的分片）
            if let AgentOutputChunk::Content { content } = &chunk {
                result_content.push_str(content);
            }

            // 遇到 Stopped 直接中断
            if let AgentOutputChunk::Stopped { reason } = &chunk {
                tracing::info!(reason = reason.as_str(), "run_agent stopped by user");
                return Ok(result_content);
            }
        }

        tracing::info!(output_len = result_content.len(), "run_agent finished");

        Ok(result_content)
    };

    match session_span {
        Some(span) => fut.instrument(span).await,
        None => fut.await,
    }
}

/// 从 `AgentOutputChunk` 中提取流式显示文本（仅 Content 产生文本）
fn extract_chunk_text(chunk: &AgentOutputChunk) -> String {
    match chunk {
        AgentOutputChunk::Content { content } => content.clone(),
        _ => String::new(),
    }
}
