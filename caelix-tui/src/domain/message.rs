use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub is_streaming: bool,
}

impl Message {
    pub fn new_user(id: MessageId, content: impl Into<String>) -> Self {
        Self {
            id,
            role: MessageRole::User,
            content: content.into(),
            created_at: Utc::now(),
            is_streaming: false,
        }
    }

    pub fn new_assistant(id: MessageId, content: impl Into<String>) -> Self {
        Self {
            id,
            role: MessageRole::Assistant,
            content: content.into(),
            created_at: Utc::now(),
            is_streaming: false,
        }
    }

    pub fn new_assistant_streaming(id: MessageId) -> Self {
        Self {
            id,
            role: MessageRole::Assistant,
            content: String::new(),
            created_at: Utc::now(),
            is_streaming: true,
        }
    }
}
