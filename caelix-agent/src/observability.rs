//! 可观测性模块 — 将 Agent 运行过程中的事件转发到外部观察者（消息总线、用量追踪器）。
//!
//! 本模块中的所有操作都是"即发即弃"的：任何失败仅记录日志，不会影响核心 Agent 运行流程。
//! 这确保了消息总线不可用、用量追踪器写入失败等外部功能故障不会阻塞或中断 Agent 的核心循环。

use std::sync::Arc;

use caelix_api::AgentOutputChunk;
use caelix_api::context::{ContextProvider, RuntimeContext, try_caelix_context};
use caelix_api::message::{AgentMessage, AgentMessageType};
use caelix_api::provider::UsageRecord;

/// 观察者上下文 — 封装 Agent 运行期间所有外部观察者（消息总线、用量追踪器）的访问。
///
/// 在 `run_agent` 开始时创建一次，后续对每个 chunk 调用 [`dispatch_chunk`]。
/// 若 `RuntimeContext` 或 `CaelixContext` 未初始化，所有观察者操作被静默跳过。
pub struct ObserverContext {
    session_id: String,
    request_id: String,
    span_id: String,
    trace_id: String,
    agent_name: Option<String>,
    provider_name: String,
    model_name: String,
    /// 全局上下文；若未初始化则为 None，所有转发操作被跳过
    caelix_ctx: Option<Arc<dyn ContextProvider>>,
}

impl ObserverContext {
    /// 从当前 task_local 的 `RuntimeContext` 和全局 `CaelixContext` 创建观察者上下文。
    pub fn from_current(
        agent_name: Option<String>,
        provider_name: String,
        model_name: String,
    ) -> Self {
        let (session_id, request_id, span_id, trace_id) = match RuntimeContext::try_current() {
            Some(ctx) => (
                ctx.get_session_id().to_string(),
                ctx.get_request_id().to_string(),
                ctx.get_span_id().to_string(),
                ctx.get_trace_id().to_string(),
            ),
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };

        Self {
            session_id,
            request_id,
            span_id,
            trace_id,
            agent_name,
            provider_name,
            model_name,
            caelix_ctx: try_caelix_context(),
        }
    }

    /// 将一个 Agent 输出分片转发到所有外部观察者。
    ///
    /// 包括：
    /// - 消息总线：将分片映射为 `AgentMessage` 并发送
    /// - 用量追踪器：在 `Finish` 分片时记录 token 用量
    ///
    /// 所有操作均为即发即弃，失败仅记录日志。
    pub async fn dispatch_chunk(&self, chunk: &AgentOutputChunk) {
        let Some(ctx) = &self.caelix_ctx else {
            return;
        };

        self.forward_to_message_bus(ctx, chunk);
        self.record_usage(ctx, chunk).await;
    }

    /// 将分片映射为 `AgentMessage` 并发送到消息总线。
    fn forward_to_message_bus(&self, ctx: &Arc<dyn ContextProvider>, chunk: &AgentOutputChunk) {
        let msg_type = match chunk {
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
                Some(format!("[工具调用] {}({}): {}", tool_call_id, name, arguments)),
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
            AgentOutputChunk::Finish { .. } => Some((AgentMessageType::Event, None)),
            AgentOutputChunk::Stopped { reason } => Some((
                AgentMessageType::Event,
                Some(format!("[已停止] {}", reason)),
            )),
        };

        if let Some((msg_type, payload)) = msg_type {
            let content = match (&msg_type, payload) {
                (AgentMessageType::Msg, Some(payload)) => payload,
                (AgentMessageType::Chunk, _) => extract_chunk_text(chunk),
                (AgentMessageType::Event, Some(desc)) => desc,
                (AgentMessageType::Event, None) => String::new(),
                _ => String::new(),
            };

            let agent_msg = AgentMessage {
                session_id: self.session_id.clone(),
                request_id: self.request_id.clone(),
                span_id: self.span_id.clone(),
                trace_id: self.trace_id.clone(),
                r#type: msg_type,
                timestamp: chrono::Utc::now(),
                content,
                agent_name: self.agent_name.clone(),
                usage: None,
            };
            if let Err(err) = ctx.message_bus().send_agent(agent_msg) {
                tracing::warn!(error = %err, "failed to send agent message");
            }
        }

        // Finish / Stopped 时追加发送 ChunkEnd，通知消费者清空请求缓冲
        let (need_chunk_end, end_usage) = match chunk {
            AgentOutputChunk::Finish { usage, .. } => (true, usage.clone()),
            AgentOutputChunk::Stopped { .. } => (true, None),
            _ => (false, None),
        };

        if need_chunk_end {
            let end_msg = AgentMessage {
                session_id: self.session_id.clone(),
                request_id: self.request_id.clone(),
                span_id: self.span_id.clone(),
                trace_id: self.trace_id.clone(),
                r#type: AgentMessageType::ChunkEnd,
                timestamp: chrono::Utc::now(),
                content: String::new(),
                agent_name: self.agent_name.clone(),
                usage: end_usage,
            };
            if let Err(err) = ctx.message_bus().send_agent(end_msg) {
                tracing::warn!(error = %err, "failed to send chunk end message");
            }
        }
    }

    /// 在 `Finish` 分片时将 token 用量记录到追踪器。
    async fn record_usage(&self, ctx: &Arc<dyn ContextProvider>, chunk: &AgentOutputChunk) {
        let AgentOutputChunk::Finish { usage: Some(u), .. } = chunk else {
            return;
        };

        let Some(tracker) = ctx.usage_tracker() else {
            return;
        };

        let _ = tracker
            .accumulate(UsageRecord {
                session_id: self.session_id.clone(),
                request_id: self.request_id.clone(),
                trace_id: self.trace_id.clone(),
                provider: self.provider_name.clone(),
                model: self.model_name.clone(),
                agent: self.agent_name.clone(),
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                reasoning_tokens: u.reasoning_tokens,
                cache_hit_tokens: u.cache_hit_tokens,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
            .await;
    }
}

/// 从 `AgentOutputChunk` 中提取流式显示文本（仅 Content 产生文本）
fn extract_chunk_text(chunk: &AgentOutputChunk) -> String {
    match chunk {
        AgentOutputChunk::Content { content } => content.clone(),
        _ => String::new(),
    }
}
