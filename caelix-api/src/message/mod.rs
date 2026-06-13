//! 消息类型定义模块

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Agent 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentMessageType {
    Chunk,
    Msg,
    ChunkEnd,
    Event,
    /// 工具调用需人工审批：携带 tool_call_id、审批类型与参数
    ManualApproval,
}

/// Agent 消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub session_id: String,
    pub request_id: String,
    pub span_id: String,
    #[serde(default)]
    pub trace_id: String,
    pub r#type: AgentMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    #[serde(default)]
    pub agent_name: Option<String>,
}

/// 通知消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationType {
    Info,
    Error,
    Warning,
    Success,
}

/// 通知消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub session_id: String,
    pub r#type: NotificationType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
}

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
    pub task_id: String,
    pub session_id: String,
    pub r#type: TaskMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub result: Option<String>, // 任务执行结果
}

/// Todo 任务触发消息（用于外部触发 Todo 任务状态变更）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoTriggerMessage {
    pub task_id: String,
    pub session_id: String,
    pub action: TodoTriggerAction,
    pub result: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Todo 任务触发动作
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TodoTriggerAction {
    Complete,     // 标记为完成
    Fail(String), // 标记为失败，附带原因
    Cancel,       // 取消任务
}

// ==================== MessageBus Trait ====================

/// 消息总线抽象 Trait
///
/// 提供 Agent 消息、通知消息、任务消息的发送与订阅接口。
/// 具体实现位于 `caelix-message` 包中。
#[async_trait]
pub trait MessageBusTrait: Send + Sync {
    /// 发送 Agent 消息
    fn send_agent(&self, msg: AgentMessage) -> Result<(), String>;

    /// 发送通知消息
    fn send_notification(&self, msg: NotificationMessage) -> Result<(), String>;

    /// 发送任务消息
    fn send_task(&self, msg: TaskMessage) -> Result<(), String>;
}

// 方便作为 Arc<dyn MessageBusTrait> 使用
impl std::fmt::Debug for dyn MessageBusTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageBusTrait").finish()
    }
}

// ==================== SessionManager Trait ====================

use futures::Stream;
use std::pin::Pin;

/// 会话管理器抽象 Trait
///
/// 管理会话状态、Agent 消息流、通知消息流、任务消息流。
/// 具体实现位于 `caelix-message` 包中。
#[async_trait]
pub trait SessionManagerTrait: Send + Sync {
    /// 订阅 Agent 消息流
    async fn subscribe_agent(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<AgentMessage>,
            Pin<Box<dyn Stream<Item = Result<AgentMessage, String>> + Send>>,
        ),
        String,
    >;

    /// 订阅通知消息流
    async fn subscribe_notification(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<NotificationMessage>,
            Pin<Box<dyn Stream<Item = Result<NotificationMessage, String>> + Send>>,
        ),
        String,
    >;

    /// 订阅任务消息流
    async fn subscribe_task(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<TaskMessage>,
            Pin<Box<dyn Stream<Item = Result<TaskMessage, String>> + Send>>,
        ),
        String,
    >;

    /// 获取当前 Session 状态
    async fn get_session_state(&self, session_id: &str) -> String;

    /// 检查会话是否存在
    async fn session_exists(&self, session_id: &str) -> bool;

    /// 获取所有会话 ID
    async fn list_sessions(&self) -> Vec<String>;

    /// 获取消息总线
    fn bus(&self) -> Arc<dyn MessageBusTrait>;
}

// 方便作为 Arc<dyn SessionManagerTrait> 使用
impl std::fmt::Debug for dyn SessionManagerTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManagerTrait").finish()
    }
}
