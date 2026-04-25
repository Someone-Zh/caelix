use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub session_id: String,
    pub span_id: String,
    pub r#type: TaskMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}

impl TaskMessage {
    /// 生成唯一的 span_id
    pub fn generate_span_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
