pub mod agent;
pub mod tool;
pub mod provider;


// 导出所有子模块内容
pub use provider::*;
pub use tool::*;

// 导出通用依赖（方便全局使用）
pub use serde::{Deserialize, Serialize};

// 通用类型定义
pub type MessageId = String; 

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