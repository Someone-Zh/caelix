//! Caelix Config - 配置中心模块
//!
//! 包含所有管理器（Manager）和配置加载器，负责初始化和管理系统的各个组件。

use std::env;
use std::path::PathBuf;

pub mod agents_loader;
pub mod commands_loader;
pub mod managers;
pub mod provider_loader;
pub mod skills_loader;
pub mod tools_loader;

pub use managers::*;

/// 环境变量配置结构体
/// 统一管理所有基于环境变量的配置项
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// CAELIX_HOME 目录路径
    pub caelix_home: PathBuf,
    /// Debug 模式是否启用
    pub debug_enabled: bool,
}

impl EnvConfig {
    /// 从环境变量创建配置实例
    pub fn new() -> Self {
        Self {
            caelix_home: Self::get_caelix_home(),
            debug_enabled: Self::is_debug_enabled(),
        }
    }

    /// 获取 CAELIX_HOME 路径
    /// 优先读取 CAELIX_HOME 环境变量，否则使用 ~/.caelix
    fn get_caelix_home() -> PathBuf {
        env::var("CAELIX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut home_dir = dirs::home_dir().expect("无法获取用户主目录");
                home_dir.push(".caelix");
                home_dir
            })
    }

    /// 检查 Debug 模式是否启用
    /// 读取 CAELIX_DEBUG 环境变量，值为 "true" 或 "1" 时启用
    fn is_debug_enabled() -> bool {
        env::var("CAELIX_DEBUG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self::new()
    }
}

// 实现 caelix-api 中定义的 EnvConfigTrait
impl caelix_api::context::EnvConfigTrait for EnvConfig {
    fn caelix_home(&self) -> &std::path::Path {
        &self.caelix_home
    }

    fn debug_enabled(&self) -> bool {
        self.debug_enabled
    }
}
