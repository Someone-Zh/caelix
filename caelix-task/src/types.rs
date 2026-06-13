// src/runtime/task/types.rs

// 从 caelix-api 导入核心任务类型（TaskMeta 也已在 api 层完整定义）
pub use caelix_api::task::{Runnable, TaskId, TaskKind, TaskMeta, TaskStatus};

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
