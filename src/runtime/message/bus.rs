use crate::runtime::message::agent_message::AgentMessage;
use crate::runtime::message::notification_message::NotificationMessage;
use crate::runtime::message::task_message::TaskMessage;
use tokio::sync::broadcast; 

#[derive(Debug, Clone)]
pub struct MessageBus {
    agent_sender: broadcast::Sender<AgentMessage>,
    notification_sender: broadcast::Sender<NotificationMessage>,
    task_sender: broadcast::Sender<TaskMessage>,
}

impl MessageBus {
    pub fn new(capacity: usize) -> Self {
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
    pub fn send_agent(&self, msg: AgentMessage) -> Result<(), broadcast::error::SendError<AgentMessage>> {
        self.agent_sender.send(msg)?;
        Ok(())
    }

    /// 发送通知消息
    pub fn send_notification(&self, msg: NotificationMessage) -> Result<(), broadcast::error::SendError<NotificationMessage>> {
        self.notification_sender.send(msg)?;
        Ok(())
    }

    /// 发送任务消息
    pub fn send_task(&self, msg: TaskMessage) -> Result<(), broadcast::error::SendError<TaskMessage>> {
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