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

/// 任务元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub id: TaskId,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub payload: String,
}
