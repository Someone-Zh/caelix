//! 消息类型定义模块

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Agent 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentMessageType {
    Chunk,
    Msg,
    ChunkEnd,
}

/// Agent 消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub session_id: String,
    pub request_id: String,
    pub span_id: String,
    pub r#type: AgentMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    #[serde(default)]
    pub agent_name: Option<String>,
}

/// 通知消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationType {
    Info,
    Error,
    Warning,
    Success,
}

/// 通知消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub session_id: String,
    pub r#type: NotificationType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}

/// 任务消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskMessageType {
    Started,
    Completed,
    Failed,
    Progress,
}

/// 任务消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub task_id: String,
    pub session_id: String,
    pub r#type: TaskMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}
