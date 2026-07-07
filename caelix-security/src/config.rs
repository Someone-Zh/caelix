use serde::{Deserialize, Serialize};

/// 安全配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// 路径安全配置
    #[serde(default)]
    pub path: PathSecurityConfig,
    /// URL 安全配置
    #[serde(default)]
    pub url: UrlSecurityConfig,
    /// 命令安全配置
    #[serde(default)]
    pub command: CommandSecurityConfig,
}

/// 路径安全配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathSecurityConfig {
    /// 允许的路径列表
    #[serde(default)]
    pub include: Vec<String>,
    /// 排除的路径列表
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// URL 安全配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UrlSecurityConfig {
    /// 允许的 URL 模式列表
    #[serde(default)]
    pub include: Vec<String>,
    /// 排除的 URL 模式列表
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// 命令安全配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandSecurityConfig {
    /// 允许执行的命令列表，支持精确命令名、绝对路径和 glob 模式
    #[serde(default)]
    pub include: Vec<String>,
    /// 禁止执行的命令列表，优先级高于 include
    #[serde(default)]
    pub exclude: Vec<String>,
}
