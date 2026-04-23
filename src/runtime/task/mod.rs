// src/runtime/task/mod.rs
//! 任务管理模块
//! 提供异步任务调度和执行功能
#![allow(dead_code)] // 部分API为将来扩展预留

pub mod manager;
pub mod persistence;
pub mod scheduler;
pub mod types;

pub use manager::TaskManager;
#[allow(unused_imports)] // 公共API导出
pub use persistence::{FilePersistence, TaskPersistence};
#[allow(unused_imports)] // 公共API导出
pub use types::{Runnable, RunnableFactory, TaskId, TaskKind, TaskMeta, TaskStatus};
