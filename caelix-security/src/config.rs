use serde::{Deserialize, Serialize};

/// 安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// 路径安全配置
    #[serde(default)]
    pub path: PathSecurityConfig,
    /// URL 安全配置
    #[serde(default)]
    pub url: UrlSecurityConfig,
}

/// 路径安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSecurityConfig {
    /// 允许的路径列表
    #[serde(default)]
    pub include: Vec<String>,
    /// 排除的路径列表
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// URL 安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlSecurityConfig {
    /// 允许的 URL 模式列表
    #[serde(default)]
    pub include: Vec<String>,
    /// 排除的 URL 模式列表
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            path: PathSecurityConfig::default(),
            url: UrlSecurityConfig::default(),
        }
    }
}

impl Default for PathSecurityConfig {
    fn default() -> Self {
        Self {
            include: vec![],  // 默认不允许任何路径
            exclude: vec![],
        }
    }
}

impl Default for UrlSecurityConfig {
    fn default() -> Self {
        Self {
            include: vec![],  // 默认不允许任何 URL
            exclude: vec![],
        }
    }
}
