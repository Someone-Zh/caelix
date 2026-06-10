//! Caelix API - 核心定义层
//!
//! 包含所有 trait、类型、错误定义，供其他包和外部使用。

pub mod agent;
pub mod tool;
pub mod provider;
pub mod message;
pub mod task;
pub mod context;
pub mod hooks;
pub mod commands;
pub mod error;
pub mod utils;
pub mod managers;

// 重新导出常用类型
pub use agent::*;
pub use tool::*;
pub use provider::*;
pub use message::*;
pub use task::*;
pub use context::*;
pub use hooks::*;
pub use commands::*;
pub use error::*;
pub use utils::*;
pub use managers::*;

// 重新导出 ChatMessage 以便其他包使用
pub use provider::ChatMessage;
