//! API 类型定义
//! 这些类型是公共API的一部分，可能被外部使用
#![allow(dead_code)]

use caelix_api::message::{AgentMessage, NotificationMessage};
use caelix_api::task::TaskMeta;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 聊天请求
#[derive(Debug, Deserialize, Clone)]
pub struct ChatRequest {
    pub session_id: String,
    /// 若为 None，则视为 "继续" 或 "恢复流程" 触发：
    /// 此时若会话最后一条消息是 Assistant 且含 tool_calls，则进入 resume 路径。
    pub message: Option<String>,
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
    pub messages: Vec<AgentMessage>,
}

/// 会话通知列表响应
#[derive(Debug, Serialize)]
pub struct SessionNotificationsResponse {
    pub notifications: Vec<NotificationMessage>,
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
    pub summary: String, // 首次输入的前15个字符
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
