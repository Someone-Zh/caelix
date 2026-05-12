use crate::enhancement::hooks::{AgentHook, HookCapability, MessageUpdateContext};
use crate::runtime::message::agent_message::{AgentMessage, AgentMessageType};
use async_trait::async_trait;
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
        // 获取最新的消息（最后一条）
        if let Some(latest_msg) = ctx.messages.last() {
            // 将整个 ChatMessage 序列化为 JSON 字符串，保留角色、工具调用等完整信息
            let content_json = match serde_json::to_string(latest_msg) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Warning: Failed to serialize ChatMessage to JSON: {}", e);
                    // 降级处理：只保存 content 字段
                    latest_msg.content.clone()
                }
            };

            // 创建 AgentMessage
            let agent_msg = AgentMessage {
                session_id: ctx.base.session_id.clone(),
                span_id: ctx.base.span_id.clone(),
                r#type: AgentMessageType::Msg,
                timestamp: Utc::now(),
                content: content_json,
            };

            // 通过 RuntimeContext 获取 CaelixContext 和 MessageBus
            if let Ok(runtime_ctx) = std::panic::catch_unwind(|| {
                crate::runtime::context::RuntimeContext::current()
            }) {
                let caelix_context = runtime_ctx.get_caelix_context();
                
                // 发送消息到消息总线
                // SessionManager 的存储消费者会自动持久化 Msg 类型的消息
                if let Err(e) = caelix_context.message_bus.send_agent(agent_msg) {
                    eprintln!("Warning: Failed to send message to bus: {}", e);
                }
            } else {
                eprintln!("Warning: No runtime context available for message bus hook");
            }
        }

        Ok(())
    }
}
