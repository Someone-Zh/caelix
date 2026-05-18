//! Caelix Config - 配置中心模块
//!
//! 包含所有管理器（Manager）和配置加载器，负责初始化和管理系统的各个组件。

pub mod managers;
pub mod context;
pub mod provider_loader;
pub mod tools_loader;
pub mod agents_loader;
pub mod skills_loader;
pub mod commands_loader;

pub use context::CaelixContext;
pub use managers::*;
