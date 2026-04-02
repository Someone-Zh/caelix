mod llm;
mod agent;
mod tool;

pub use llm::*;
pub use agent::*;
pub use tool::*;

// 通用类型定义
type MessageId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

// 错误定义
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    LlmError(String),
    #[error("Tool error: {0}")]
    ToolError(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Invalid tool response: {0}")]
    InvalidToolResponse(String),
    #[error("Task error: {0}")]
    TaskError(String),
}

// 导出必要的依赖
pub use serde::{Deserialize, Serialize};
pub use serde_json;
pub use std::collections::HashMap;
pub use uuid;
pub use chrono;