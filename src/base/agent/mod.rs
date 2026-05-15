//! Agent 核心模块
#![allow(dead_code)] // 部分API为将来扩展预留

// 导出所有子模块
mod types;
mod traits;
mod converter;
mod tool_executor;
mod loop_runner;
mod executor;  // 新增

// 对外统一导出核心类型
pub use types::{AgentOutputChunk, AgentSpec};
pub use traits::Agent;
pub use executor::execute_agent_with_messaging;  // 新增导出