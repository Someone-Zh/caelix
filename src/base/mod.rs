pub mod agent;
pub mod tool;
pub mod provider;


// 导出所有子模块内容
pub use provider::*;
pub use tool::*;

// 错误定义
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // 部分变体为将来扩展预留
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