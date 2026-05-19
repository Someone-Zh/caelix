//! Caelix Config - 配置中心模块
//!
//! 包含所有管理器（Manager）和配置加载器，负责初始化和管理系统的各个组件。

use std::env;
use std::path::PathBuf;

pub mod managers;
pub mod provider_loader;
pub mod tools_loader;
pub mod agents_loader;
pub mod skills_loader;
pub mod commands_loader;

pub use managers::*;

/// 从环境变量或默认位置获取CAELIX_HOME路径
pub fn get_caelix_home() -> PathBuf {
    env::var("CAELIX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut home_dir = dirs::home_dir().expect("无法获取用户主目录");
            home_dir.push(".caelix");
            home_dir
        })
}
