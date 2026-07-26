use crate::domain::{
    Message, MessageId, Notification, Task,
};
use crate::infrastructure::{
    ChatService, MockServices, NotificationService, TaskService,
};

pub struct AppService {
    next_message_id: u64,
    messages: Vec<Message>,
    tasks: Vec<Task>,
    notifications: Vec<Notification>,
    chat_service: Box<dyn ChatService>,
    task_service: Box<dyn TaskService>,
    notification_service: Box<dyn NotificationService>,
    stream_chunks: Option<Vec<String>>,
    current_stream_index: usize,
    streaming_message_id: Option<MessageId>,
}

impl AppService {
    pub fn new(
        chat_service: Box<dyn ChatService>,
        task_service: Box<dyn TaskService>,
        notification_service: Box<dyn NotificationService>,
    ) -> Self {
        Self {
            next_message_id: 1,
            messages: Vec::new(),
            tasks: Vec::new(),
            notifications: Vec::new(),
            chat_service,
            task_service,
            notification_service,
            stream_chunks: None,
            current_stream_index: 0,
            streaming_message_id: None,
        }
    }

    pub fn new_mock() -> Self {
        let mock = MockServices::new();
        Self::new(Box::new(mock), Box::new(MockServices::new()), Box::new(MockServices::new()))
    }

    fn next_message_id(&mut self) -> MessageId {
        let id = MessageId(self.next_message_id);
        self.next_message_id += 1;
        id
    }

    pub fn get_messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn get_tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn get_notifications(&self) -> &[Notification] {
        &self.notifications
    }

    pub fn is_streaming(&self) -> bool {
        self.stream_chunks.is_some()
    }

    pub async fn send_user_message(&mut self, content: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let user_id = self.next_message_id();
        self.messages.push(Message::new_user(user_id, content.clone()));

        let assistant_id = self.next_message_id();
        self.messages.push(Message::new_assistant_streaming(assistant_id));
        self.streaming_message_id = Some(assistant_id);

        let chunks = self.chat_service.stream_reply(&content).await?;
        self.stream_chunks = Some(chunks);
        self.current_stream_index = 0;

        Ok(())
    }

    pub fn tick_stream(&mut self) -> bool {
        let (streaming_id, chunks, index) = match (self.streaming_message_id, self.stream_chunks.as_ref()) {
            (Some(id), Some(chunks)) => (id, chunks.clone(), self.current_stream_index),
            _ => return false,
        };

        if index >= chunks.len() {
            if let Some(msg) = self.messages.iter_mut().find(|m| m.id == streaming_id) {
                msg.is_streaming = false;
            }
            self.stream_chunks = None;
            self.streaming_message_id = None;
            self.current_stream_index = 0;
            return false;
        }

        let chunk = &chunks[index];
        if let Some(msg) = self.messages.iter_mut().find(|m| m.id == streaming_id) {
            msg.content.push_str(chunk);
        }
        self.current_stream_index += 1;
        true
    }

    pub async fn refresh_tasks(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.tasks = self.task_service.list_tasks().await?;
        Ok(())
    }

    pub async fn refresh_notifications(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.notifications = self.notification_service.list_notifications().await?;
        Ok(())
    }
}
