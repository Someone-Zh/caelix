//! Agent 核心模块
#![allow(dead_code)] // 部分API为将来扩展预留

// 从 caelix-api 重新导出类型
pub use caelix_api::agent::{AgentOutputChunk, AgentSpec};

// 导出所有子模块
mod converter;
mod tool_executor;
mod loop_runner;
mod executor;

// 对外统一导出核心函数
pub use executor::execute_agent_with_messaging;