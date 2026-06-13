use crate::config::SecurityConfig;
use crate::path_checker::PathChecker;
use crate::url_checker::UrlChecker;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// 安全错误类型
#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Path traversal detected")]
    PathTraversalDetected,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// 统一的安全检测器
pub struct SecurityChecker {
    config: Arc<RwLock<SecurityConfig>>,
    path_checker: Arc<RwLock<PathChecker>>,
    url_checker: Arc<RwLock<UrlChecker>>,
}

impl std::fmt::Debug for SecurityChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityChecker")
            .field("config", &"<locked>")
            .finish()
    }
}

impl SecurityChecker {
    /// 创建新的 SecurityChecker 实例
    pub fn new(config: SecurityConfig) -> Self {
        let path_config = config.path.clone();
        let url_config = config.url.clone();

        Self {
            config: Arc::new(RwLock::new(config)),
            path_checker: Arc::new(RwLock::new(PathChecker::new(path_config))),
            url_checker: Arc::new(RwLock::new(UrlChecker::new(url_config))),
        }
    }

    /// 检查路径是否安全
    pub async fn is_path_safe(&self, path: &str) -> bool {
        let checker = self.path_checker.read().await;
        checker.is_safe(path)
    }

    /// 检查 URL 是否安全
    pub async fn is_url_safe(&self, url: &str) -> bool {
        let checker = self.url_checker.read().await;
        checker.is_safe(url)
    }

    /// 添加允许路径并持久化
    pub async fn add_path_include(&self, path: String) -> Result<(), SecurityError> {
        // 更新内存中的配置
        {
            let mut checker = self.path_checker.write().await;
            checker.add_include(path.clone());
        }

        // 更新全局配置
        {
            let mut config = self.config.write().await;
            if !config.path.include.contains(&path) {
                config.path.include.push(path);
            }
        }

        // TODO: 持久化到文件(由 caelix-config 负责)
        Ok(())
    }

    /// 添加排除路径并持久化
    pub async fn add_path_exclude(&self, path: String) -> Result<(), SecurityError> {
        {
            let mut checker = self.path_checker.write().await;
            checker.add_exclude(path.clone());
        }

        {
            let mut config = self.config.write().await;
            if !config.path.exclude.contains(&path) {
                config.path.exclude.push(path);
            }
        }

        Ok(())
    }

    /// 添加允许 URL 模式并持久化
    pub async fn add_url_include(&self, pattern: String) -> Result<(), SecurityError> {
        {
            let mut checker = self.url_checker.write().await;
            checker.add_include(pattern.clone());
        }

        {
            let mut config = self.config.write().await;
            if !config.url.include.contains(&pattern) {
                config.url.include.push(pattern);
            }
        }

        Ok(())
    }

    /// 添加排除 URL 模式并持久化
    pub async fn add_url_exclude(&self, pattern: String) -> Result<(), SecurityError> {
        {
            let mut checker = self.url_checker.write().await;
            checker.add_exclude(pattern.clone());
        }

        {
            let mut config = self.config.write().await;
            if !config.url.exclude.contains(&pattern) {
                config.url.exclude.push(pattern);
            }
        }

        Ok(())
    }

    /// 获取当前配置
    pub async fn get_config(&self) -> SecurityConfig {
        self.config.read().await.clone()
    }

    /// 重新加载配置
    pub async fn reload_config(&self, new_config: SecurityConfig) {
        let path_config = new_config.path.clone();
        let url_config = new_config.url.clone();

        *self.config.write().await = new_config;
        *self.path_checker.write().await = PathChecker::new(path_config);
        *self.url_checker.write().await = UrlChecker::new(url_config);
    }
}

// 实现 caelix-api 中定义的 SecurityCheckerTrait
#[async_trait::async_trait]
impl caelix_api::context::SecurityCheckerTrait for SecurityChecker {
    async fn check_path(&self, path: &str) -> Result<(), String> {
        if self.is_path_safe(path).await {
            Ok(())
        } else {
            Err(format!("Path '{}' is not allowed (security policy)", path))
        }
    }

    async fn check_url(&self, url: &str) -> Result<(), String> {
        if self.is_url_safe(url).await {
            Ok(())
        } else {
            Err(format!("URL '{}' is not allowed (security policy)", url))
        }
    }
}
