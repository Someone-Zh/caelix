//! Caelix Config - 配置中心模块
//!
//! 包含所有管理器（Manager）和配置加载器，负责初始化和管理系统的各个组件。

use caelix_api::logging::LogConfig;
use std::env;
use std::path::{Path, PathBuf};

pub mod agents_loader;
pub mod commands_loader;
pub mod managers;
pub mod provider_loader;
pub mod skills_loader;
pub mod tools_loader;

pub use managers::*;

/// 项目配置目录名称常量（与 CAELIX_HOME 下一致）
pub const SKILLS_DIR: &str = "skills";
pub const COMMANDS_DIR: &str = "commands";
pub const AGENTS_DIR: &str = "agents";

/// 项目配置路径常量（相对于项目根目录）
pub const PROJECT_CONFIG_PATHS: [&str; 3] = [SKILLS_DIR, COMMANDS_DIR, AGENTS_DIR];

/// 完整的根配置（可从 JSON 反序列化）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct RootConfig {
    #[serde(default)]
    logging: Option<LogConfig>,
}

/// 环境变量配置结构体
///
/// 统一管理所有基于环境变量的配置项，并从 `$CAELIX_HOME/config.json` （如果存在）
/// 加载可选项。
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// CAELIX_HOME 目录路径
    pub caelix_home: PathBuf,
    /// Debug 模式是否启用
    pub debug_enabled: bool,
    /// 日志配置
    pub log: LogConfig,
}

impl EnvConfig {
    /// 从环境变量 + 配置文件创建实例（同步版本，用于启动阶段）
    pub fn new() -> Self {
        let caelix_home = Self::get_caelix_home();
        let debug_enabled = Self::is_debug_enabled();

        // 默认日志配置（以 caelix_home 为基准）
        let mut log = LogConfig::default().with_caelix_home(&caelix_home);

        // 尝试从 config.json 加载（若存在）
        let config_path = caelix_home.join("config.json");
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str::<RootConfig>(&content) {
                    Ok(root) => {
                        if let Some(loaded_log) = root.logging {
                            log = loaded_log;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %config_path.display(),
                            error = %e,
                            "解析配置文件失败，使用默认配置"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %config_path.display(),
                        error = %e,
                        "读取配置文件失败，使用默认配置"
                    );
                }
            }
        }

        Self {
            caelix_home,
            debug_enabled,
            log,
        }
    }

    /// 从环境变量 + 配置文件异步创建实例（避免阻塞 async runtime）
    pub async fn new_async() -> Self {
        let caelix_home = Self::get_caelix_home();
        let debug_enabled = Self::is_debug_enabled();

        let mut log = LogConfig::default().with_caelix_home(&caelix_home);

        let config_path = caelix_home.join("config.json");
        if config_path.exists() {
            match tokio::fs::read_to_string(&config_path).await {
                Ok(content) => match serde_json::from_str::<RootConfig>(&content) {
                    Ok(root) => {
                        if let Some(loaded_log) = root.logging {
                            log = loaded_log;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %config_path.display(),
                            error = %e,
                            "解析配置文件失败，使用默认配置"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %config_path.display(),
                        error = %e,
                        "读取配置文件失败，使用默认配置"
                    );
                }
            }
        }

        Self {
            caelix_home,
            debug_enabled,
            log,
        }
    }

    /// 获取 CAELIX_HOME 路径
    /// 优先读取 CAELIX_HOME 环境变量，否则使用 ~/.caelix
    fn get_caelix_home() -> PathBuf {
        env::var("CAELIX_HOME")
            .map(PathBuf::from)
            .ok()
            .unwrap_or_else(|| {
                if let Some(mut home_dir) = dirs::home_dir() {
                    home_dir.push(".caelix");
                    home_dir
                } else {
                    tracing::warn!("无法获取用户主目录，使用当前目录作为 CAELIX_HOME");
                    PathBuf::from(".caelix")
                }
            })
    }

    /// 检查 Debug 模式是否启用
    /// 读取 CAELIX_DEBUG 环境变量，值为 "true" 或 "1" 时启用
    fn is_debug_enabled() -> bool {
        env::var("CAELIX_DEBUG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }

    /// 读取配置文件中一段（用于项目级别的覆盖，暂未启用）
    #[allow(dead_code)]
    fn load_overlay_from(_path: &Path) -> Option<RootConfig> {
        None
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
