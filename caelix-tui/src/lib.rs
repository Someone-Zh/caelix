//! Caelix TUI - 终端用户界面后端
//!
//! 提供基于 Ratatui 的终端用户界面

/// TUI 应用状态和核心逻辑
pub mod state;
/// 命令处理逻辑
pub mod commands;
/// 视图渲染逻辑
pub mod views;
/// 事件处理
pub mod events;
/// 主循环运行器
pub mod runner;

pub use runner::run_tui;
