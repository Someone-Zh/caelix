// src/runtime/task/types.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// 从 caelix-api 导入核心任务类型
pub use caelix_api::task::{Runnable, TaskId, TaskKind, TaskStatus};

// ==================== 任务元数据 (用于持久化) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub task_id: TaskId,
    pub session_id: String,
    pub span_id: String,
    pub tool_call_id: Option<String>,
    pub task_name: Option<String>,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub progress: Option<f32>,  // 任务进度 0.0-1.0
    pub result: Option<String>, // 任务执行结果
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
