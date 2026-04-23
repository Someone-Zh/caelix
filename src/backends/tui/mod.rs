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

pub use events::EventHandler;
pub use runner::run_tui;
