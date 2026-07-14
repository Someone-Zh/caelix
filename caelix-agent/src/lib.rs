//! Caelix Agent - Agent 引擎
//!
//! 包含 Agent 执行器、循环运行器、工具执行器等
//!
//! 模块职责划分：
//! - `loop_agent`：核心 LLM 调用与工具执行循环
//! - `agent_runner`：核心运行器，消费流并累积结果
//! - `tool_executor`：工具批量执行
//! - `observability`：外部观察者（消息总线、用量追踪），失败不影响核心流程
//! - `security_check`：工具执行前的安全预检查，fail-closed 降级为人工审批

mod agent_runner;
pub mod converter;
pub mod loop_agent;
mod observability;
mod security_check;
pub mod tool_executor;
mod util;
pub use agent_runner::run_agent;
