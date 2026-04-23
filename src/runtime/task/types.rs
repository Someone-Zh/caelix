// src/runtime/task/types.rs
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ==================== ID 定义 ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ==================== 任务分类与状态 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskKind {
    Async,
    Once(DateTime<Utc>),
    Cron(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Scheduled,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

// ==================== 可执行任务 Trait ====================

#[async_trait]
pub trait Runnable: Send + Sync + 'static {
    async fn run(&self) -> anyhow::Result<()>;
    fn task_type(&self) -> &'static str;
    fn payload(&self) -> String;
}

// ==================== 任务元数据 (用于持久化) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub task_id: TaskId,
    pub session_id: String,
    pub span_id: String,
    pub tool_call_id: Option<String>,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub progress: Option<f32>,  // 任务进度 0.0-1.0
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
            kind,
            status: TaskStatus::Pending,
            progress: None,
            created_at: now,
            updated_at: now,
            task_type_name,
            task_payload,
        }
    }
}

// ==================== 工厂 Trait (用于恢复任务) ====================

pub type RunnableConstructor = Box<dyn Fn(String) -> Box<dyn Runnable> + Send + Sync>;

pub struct RunnableFactory {
    constructors: std::collections::HashMap<String, RunnableConstructor>,
}

impl RunnableFactory {
    pub fn new() -> Self {
        Self {
            constructors: std::collections::HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &'static str, constructor: F)
    where
        F: Fn(String) -> Box<dyn Runnable> + Send + Sync + 'static,
    {
        self.constructors
            .insert(name.to_string(), Box::new(constructor));
    }

    pub fn create(&self, name: &str, payload: String) -> Option<Box<dyn Runnable>> {
        self.constructors.get(name).map(|ctor| ctor(payload))
    }
}

impl Default for RunnableFactory {
    fn default() -> Self {
        Self::new()
    }
}