use caelix_api::message::{AgentMessage, NotificationMessage, TaskMessage};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct MessageBus {
    agent_sender: broadcast::Sender<AgentMessage>,
    notification_sender: broadcast::Sender<NotificationMessage>,
    task_sender: broadcast::Sender<TaskMessage>,
}

impl MessageBus {
    pub fn new(capacity: usize) -> Self {
        // 使用更大的容量避免消息丢失，确保至少 8192
        let capacity = capacity.max(8192);
        let (agent_sender, _) = broadcast::channel(capacity);
        let (notification_sender, _) = broadcast::channel(capacity);
        let (task_sender, _) = broadcast::channel(capacity);
        Self {
            agent_sender,
            notification_sender,
            task_sender,
        }
    }

    /// 发送 Agent 消息
    #[allow(dead_code)] // 为将来外部访问预留
    #[allow(clippy::result_large_err)]
    pub fn send_agent(
        &self,
        msg: AgentMessage,
    ) -> Result<(), broadcast::error::SendError<AgentMessage>> {
        self.agent_sender.send(msg)?;
        Ok(())
    }

    /// 发送通知消息
    #[allow(dead_code)] // 为将来外部访问预留
    pub fn send_notification(
        &self,
        msg: NotificationMessage,
    ) -> Result<(), broadcast::error::SendError<NotificationMessage>> {
        self.notification_sender.send(msg)?;
        Ok(())
    }

    /// 发送任务消息
    pub fn send_task(
        &self,
        msg: TaskMessage,
    ) -> Result<(), broadcast::error::SendError<TaskMessage>> {
        self.task_sender.send(msg)?;
        Ok(())
    }

    /// 订阅 Agent 消息
    pub fn subscribe_agent(&self) -> broadcast::Receiver<AgentMessage> {
        self.agent_sender.subscribe()
    }

    /// 订阅通知消息
    pub fn subscribe_notification(&self) -> broadcast::Receiver<NotificationMessage> {
        self.notification_sender.subscribe()
    }

    /// 订阅任务消息
    pub fn subscribe_task(&self) -> broadcast::Receiver<TaskMessage> {
        self.task_sender.subscribe()
    }
}

// ==================== 实现 MessageSender trait ====================

impl caelix_api::context::MessageSender for MessageBus {
    fn send_agent(&self, message: caelix_api::message::AgentMessage) -> Result<(), anyhow::Error> {
        // 直接使用，无需转换（类型已统一）
        Self::send_agent(self, message)
            .map_err(|e| anyhow::anyhow!("Failed to send agent message: {}", e))
    }

    fn send_task(&self, message: caelix_api::message::TaskMessage) -> Result<(), anyhow::Error> {
        // 直接使用，无需转换（类型已统一）
        Self::send_task(self, message)
            .map_err(|e| anyhow::anyhow!("Failed to send task message: {}", e))
    }
}
