//! Caelix Runtime - 运行时层
//!
//! 包含 Hook 系统、RuntimeContext 实现、命令系统等运行时功能

pub mod agent_run_manager;
pub mod context;
pub mod hooks;
pub mod id_generator;
pub mod plugins;
pub mod usage_tracker;

// 重新导出常用类型
pub use agent_run_manager::AgentRunManager;
pub use hooks::HookRegistry;
pub use id_generator::*;
pub use usage_tracker::UsageTracker;
