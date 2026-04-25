use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Agent 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentMessageType {
    Chunk,      // 流式内容块
    Msg,        // 完整消息（需持久化）
    ChunkEnd,   // 流式结束标记
}

/// Agent 消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub session_id: String,
    pub span_id: String,
    pub r#type: AgentMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}

impl AgentMessage {
    /// 生成唯一的 span_id
    pub fn generate_span_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
