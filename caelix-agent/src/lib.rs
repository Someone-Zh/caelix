//! Caelix Agent - Agent 引擎
//!
//! 包含 Agent 执行器、循环运行器、工具执行器等

pub mod executor;
pub mod loop_runner;
pub mod tool_executor;
pub mod converter;
pub mod tools;

// 重新导出常用函数
pub use executor::execute_agent_with_messaging;
pub use tools::*;
