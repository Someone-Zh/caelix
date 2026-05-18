use crate::hooks::{AgentHook, HookCapability, MessageUpdateContext};
use async_trait::async_trait;

/// MessageBusHook - 负责将消息更新发送到消息总线并持久化
/// 
/// 注意：由于循环依赖问题，此钩子将在 caelix-config 中重新实现
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

    async fn on_message_update(&self, _ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        // TODO: 恢复消息总线功能
        // 由于 RuntimeContext 现在不存储 CaelixContext，需要重新设计如何获取 MessageBus
        // 这个钩子将在 caelix-config 中重新实现，那里可以直接访问 CaelixContext
        Ok(())
    }
}
