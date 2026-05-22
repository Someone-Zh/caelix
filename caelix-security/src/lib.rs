//! Caelix Security - 安全检测模块
//!
//! 提供文件路径和 URL 访问控制功能,包括:
//! - 路径白名单/黑名单机制
//! - URL 模式匹配(支持通配符)
//! - 防止路径穿越攻击
//! - 运行时配置管理
//! - 配置持久化

pub mod config;
pub mod checker;
pub mod path_checker;
pub mod url_checker;
pub mod loader;

pub use config::*;
pub use checker::SecurityChecker;
pub use checker::SecurityError;
