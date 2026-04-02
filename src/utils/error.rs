use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// --- 基础 ID 类型 ---
pub type MessageId = String;
pub type SessionId = String;
pub type AgentId = String;
pub type ToolId = String;

// --- 统一错误定义 ---
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM 调用失败: {0}")]
    LlmError(String),
    
    #[error("工具执行失败: {0}, 原因: {1}")]
    ToolExecutionError(ToolId, String),
    
    #[error("记忆检索失败: {0}")]
    MemoryError(String),
    
    #[error("系统内部错误: {0}")]
    InternalError(String),
}

// --- 消息角色 ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    User,
    System,
    Assistant,
    ToolResult, // 工具执行结果
}