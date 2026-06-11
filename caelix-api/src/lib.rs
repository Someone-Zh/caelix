//! Caelix API - 核心定义层
//!
//! 包含所有 trait、类型、错误定义，供其他包和外部使用。

pub mod agent;
pub mod commands;
pub mod context;
pub mod error;
pub mod hooks;
pub mod managers;
pub mod message;
pub mod plugins;
pub mod provider;
pub mod task;
pub mod tool;
pub mod utils;

// 重新导出常用类型
pub use agent::*;
pub use commands::*;
pub use context::*;
pub use error::*;
pub use hooks::*;
pub use managers::*;
pub use message::*;
pub use plugins::*;
pub use provider::*;
pub use task::*;
pub use tool::*;
pub use utils::*;

// 重新导出 ChatMessage 以便其他包使用
pub use provider::ChatMessage;
