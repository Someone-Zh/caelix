//! Caelix CLI - 命令行界面后端
//!
//! 提供基于命令行的用户交互界面

pub mod runner;
pub mod commands;
pub mod input_handler;

pub use runner::run_cli;
