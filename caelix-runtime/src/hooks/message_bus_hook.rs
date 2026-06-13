use crate::hooks::{AgentHook, HookCapability, MessageUpdateContext};
use async_trait::async_trait;
use caelix_api::context::RuntimeContext;
use caelix_api::message::{AgentMessage, AgentMessageType};
use chrono::Utc;

/// MessageBusHook - 负责将消息更新发送到消息总线并持久化
pub struct MessageBusHook;

impl MessageBusHook {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentHook for MessageBusHook {
    fn name(&self) -> &str {
        "message_bus_hook"
    }

    fn capabilities(&self) -> HookCapability {
        // 只关注消息更新阶段
        HookCapability::ON_MESSAGE_UPDATE
    }

    async fn on_message_update(&self, ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        // 从全局 task_local 的 RuntimeContext 获取运行时信息（session/request/span/trace id）
        let runtime_ctx = RuntimeContext::try_current().expect(
            "message_bus_hook: RuntimeContext 未在当前协程作用域中设置，请通过 with_runtime_ctx 绑定上下文后再调用钩子",
        );

        // 按序循环处理 ctx.messages 中的每一条 ChatMessage
        for msg in ctx.messages.iter() {
            // 将整个 ChatMessage 序列化为 JSON 字符串，保留角色、工具调用等完整信息
            let content_json = match serde_json::to_string(msg) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Warning: Failed to serialize ChatMessage to JSON: {}", e);
                    // 降级处理：只保存 content 字段
                    msg.content.clone()
                }
            };

            // 创建 AgentMessage —— session/request/span/trace 全部来自 RuntimeContext
            let agent_msg = AgentMessage {
                session_id: runtime_ctx.get_session_id().to_string(),
                request_id: runtime_ctx.get_request_id().to_string(),
                span_id: runtime_ctx.get_span_id().to_string(),
                trace_id: runtime_ctx.get_trace_id().to_string(),
                r#type: AgentMessageType::Msg,
                timestamp: Utc::now(),
                content: content_json,
                agent_name: Some(ctx.agent_name.clone()),
            };

            // 通过全局 CaelixContext 获取消息总线并发送消息
            if let Some(ctx_provider) = caelix_api::context::try_caelix_context() {
                if let Err(e) = ctx_provider.message_bus().send_agent(agent_msg) {
                    eprintln!("Warning: Failed to send message to bus: {}", e);
                }
            } else {
                eprintln!("Warning: CaelixContext 尚未初始化，无法发送消息到总线");
            }
        }

        Ok(())
    }
}
