use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{Notification, Task};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("service error: {0}")]
    Other(String),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

#[async_trait]
pub trait ChatService: Send + Sync {
    async fn generate_reply(&self, user_input: &str) -> ServiceResult<String>;

    async fn stream_reply(
        &self,
        user_input: &str,
    ) -> ServiceResult<Vec<String>>;
}

#[async_trait]
pub trait TaskService: Send + Sync {
    async fn list_tasks(&self) -> ServiceResult<Vec<Task>>;
}

#[async_trait]
pub trait NotificationService: Send + Sync {
    async fn list_notifications(&self) -> ServiceResult<Vec<Notification>>;
}
