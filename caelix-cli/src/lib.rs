//! Caelix CLI - 命令行界面后端
//!
//! 提供基于命令行的用户交互界面

pub mod commands;
pub mod doc;
pub mod input_handler;
pub mod runner;

pub use runner::run_cli;
