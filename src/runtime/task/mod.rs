// src/runtime/task/mod.rs
pub mod manager;
pub mod persistence;
pub mod scheduler;
pub mod types;

pub use manager::TaskManager;
pub use persistence::{FilePersistence, TaskPersistence};
pub use types::{Runnable, RunnableFactory, TaskId, TaskKind, TaskMeta, TaskStatus};

#[cfg(test)]
pub mod test;