//! 任务类型定义模块

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskKind {
    Async,
    Once(DateTime<Utc>),
    Cron(String),
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
    async fn run(&self) -> anyhow::Result<()>;
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
