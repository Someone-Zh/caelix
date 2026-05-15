//! API 类型定义
//! 这些类型是公共API的一部分，可能被外部使用
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;
use chrono::{DateTime, Utc};
use crate::runtime::TaskMeta;

/// API 错误类型
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),
    
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Stream error: {0}")]
    StreamError(String),
}

impl ApiError {
    pub fn session_not_found(session_id: &str) -> Self {
        ApiError::SessionNotFound(session_id.to_string())
    }
    
    pub fn provider_not_found(provider: &str) -> Self {
        ApiError::ProviderNotFound(provider.to_string())
    }
    
    pub fn agent_not_found(agent: &str) -> Self {
        ApiError::AgentNotFound(agent.to_string())
    }
}

/// 从 AgentError 转换为 ApiError
impl From<crate::base::AgentError> for ApiError {
    fn from(err: crate::base::AgentError) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

/// 从 anyhow::Error 转换为 ApiError
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

/// 聊天请求
#[derive(Debug, Deserialize, Clone)]
pub struct ChatRequest {
    pub session_id: String,
    pub message: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
}

/// 会话创建响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

/// 默认配置响应
#[derive(Debug, Serialize)]
pub struct DefaultConfigResponse {
    pub default_provider: String,
    pub default_model: String,
}

/// Agent 列表响应
#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<String>,
}

/// 会话消息列表响应
#[derive(Debug, Serialize)]
pub struct SessionMessagesResponse {
    pub messages: Vec<crate::runtime::message::agent_message::AgentMessage>,
}

/// 会话通知列表响应
#[derive(Debug, Serialize)]
pub struct SessionNotificationsResponse {
    pub notifications: Vec<crate::runtime::message::notification_message::NotificationMessage>,
}

/// 任务列表响应
#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskMeta>,
}

/// 任务查询参数
#[derive(Debug, Deserialize)]
pub struct TaskQueryParams {
    pub session_id: Option<String>,
}

/// 会话摘要信息
#[derive(Debug, Serialize, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub summary: String,  // 首次输入的前15个字符
}

/// 提供者信息
#[derive(Debug, Serialize, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub llm_type: String,
    pub models: Vec<String>,
}

/// 异步聊天响应
#[derive(Debug, Serialize, Clone)]
pub struct ChatAsyncResult {
    pub request_id: String,
    pub span_id: String,
    pub session_id: String,
}
