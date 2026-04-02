use serde::{Deserialize, Serialize};

// --- 客户端请求 ---
#[derive(Debug, Deserialize)]
pub struct AgentRequest {
    pub session_id: SessionId,
    pub message: String,
    // 可选：指定使用的技能或模型
    pub skill_id: Option<String>,
    pub stream: bool, // 是否开启流式输出
}

// --- 客户端响应 ---
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub session_id: SessionId,
    pub message_id: MessageId,
    pub content: String,
    pub done: bool, // 流式模式下，done=true 表示结束
}