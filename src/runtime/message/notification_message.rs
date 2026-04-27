use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 通知消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationType {
    Info,
    Success,
    Error,
    Warning,
}

/// 通知消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub session_id: String,
    pub span_id: String,
    pub r#type: NotificationType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}
