//! 任务类型定义模块

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::AgentError;
use crate::utils;

/// 任务ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new() -> Self {
        Self(utils::generate_task_id())
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 任务分类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskKind {
    Async,
    Once(DateTime<Utc>),
    Cron(String),
    Todo, // 待办任务，完全由外部触发状态变更
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Scheduled,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// 可执行任务 Trait
#[async_trait]
pub trait Runnable: Send + Sync + 'static {
    /// 执行任务并返回结果
    ///
    /// # Returns
    /// - Ok(String): 任务执行成功，返回结果字符串
    /// - Err(AgentError): 任务执行失败，返回错误信息
    async fn run(&self) -> Result<String, AgentError>;
    fn task_type(&self) -> &'static str;
    fn payload(&self) -> String;
}

/// 任务工厂 Trait
pub trait RunnableFactory: Send + Sync {
    fn create(&self, kind: &TaskKind, payload: &str) -> Option<Box<dyn Runnable>>;
}

/// 任务元数据 (完整版本，由 caelix-task 使用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub task_id: TaskId,
    pub session_id: String,
    pub span_id: String,
    pub tool_call_id: Option<String>,
    pub task_name: Option<String>,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub progress: Option<f32>,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub task_type_name: String,
    pub task_payload: String,
}

impl TaskMeta {
    pub fn new(
        session_id: String,
        span_id: String,
        tool_call_id: Option<String>,
        task_name: Option<String>,
        kind: TaskKind,
        task_type_name: String,
        task_payload: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            task_id: TaskId::new(),
            session_id,
            span_id,
            tool_call_id,
            task_name,
            kind,
            status: TaskStatus::Pending,
            progress: None,
            result: None,
            created_at: now,
            updated_at: now,
            task_type_name,
            task_payload,
        }
    }
}

// ==================== TaskManager Trait ====================

/// 任务管理器抽象 Trait
///
/// 提供任务提交、取消、状态查询、进度更新等功能。
/// 具体实现位于 `caelix-task` 包中。
#[async_trait]
pub trait TaskManagerTrait: Send + Sync {
    /// 提交新任务
    async fn submit(
        &self,
        tool_call_id: Option<String>,
        task_name: Option<String>,
        kind: TaskKind,
        runnable: Box<dyn Runnable>,
    ) -> TaskId;

    /// 取消任务
    async fn cancel(&self, task_id: TaskId) -> bool;

    /// 获取任务状态
    async fn get_status(&self, task_id: &TaskId) -> Option<TaskMeta>;

    /// 列出任务（支持按 session 过滤）
    async fn list_tasks(&self, filter_session: Option<&str>) -> Vec<TaskMeta>;

    /// 更新任务进度
    async fn update_progress(&self, task_id: TaskId, progress: f32) -> bool;

    /// 更新 Todo 任务状态
    async fn update_todo_status(
        &self,
        task_id: TaskId,
        new_status: TaskStatus,
        result: Option<String>,
    ) -> bool;

    /// 恢复持久化任务
    async fn restore(&self) -> Result<(), String>;
}

impl std::fmt::Debug for dyn TaskManagerTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskManagerTrait").finish()
    }
}
