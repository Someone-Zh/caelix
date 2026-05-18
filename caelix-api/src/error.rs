//! 错误定义模块
//!
//! 包含所有公共错误类型

use thiserror::Error;

/// Agent 错误
#[derive(Debug, Error)]
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

/// API 错误
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl ApiError {
    pub fn session_not_found(session_id: &str) -> Self {
        Self::SessionNotFound(session_id.to_string())
    }

    pub fn provider_not_found(provider: &str) -> Self {
        Self::ProviderNotFound(provider.to_string())
    }

    pub fn agent_not_found(agent: &str) -> Self {
        Self::AgentNotFound(agent.to_string())
    }

    pub fn model_not_found(model: &str) -> Self {
        Self::InternalError(format!("Model not found: {}", model))
    }

    pub fn invalid_request(msg: &str) -> Self {
        Self::InternalError(format!("Invalid request: {}", msg))
    }

    pub fn stream_error(msg: &str) -> Self {
        Self::InternalError(format!("Stream error: {}", msg))
    }
}

/// 从 AgentError 转换为 ApiError
impl From<AgentError> for ApiError {
    fn from(err: AgentError) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

/// 消息错误
#[derive(Debug, Clone, Error)]
#[error("Message error: {message} (code: {code})")]
pub struct MessageError {
    pub code: String,
    pub message: String,
    pub stack: Option<String>,
}
