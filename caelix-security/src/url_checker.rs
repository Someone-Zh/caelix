use crate::config::UrlSecurityConfig;
use glob::Pattern;
use url::Url;

/// URL 安全检测器
pub struct UrlChecker {
    config: UrlSecurityConfig,
}

impl UrlChecker {
    /// 创建新的 UrlChecker 实例
    pub fn new(config: UrlSecurityConfig) -> Self {
        Self { config }
    }

    /// 检查 URL 是否可访问
    ///
    /// 规则:
    /// 1. 如果 URL 匹配 exclude 模式,返回 false
    /// 2. 如果 URL 匹配 include 模式,返回 true
    /// 3. 否则返回 false
    pub fn is_safe(&self, url_str: &str) -> bool {
        // 解析 URL
        let parsed_url = match Url::parse(url_str) {
            Ok(u) => u,
            Err(_) => return false, // 无效 URL
        };

        // 首先检查是否在排除列表中
        for pattern in &self.config.exclude {
            if self.matches_pattern(&parsed_url, pattern) {
                return false;
            }
        }

        // 然后检查是否在允许列表中
        for pattern in &self.config.include {
            if self.matches_pattern(&parsed_url, pattern) {
                return true;
            }
        }

        false
    }

    /// 检查 URL 是否匹配模式(支持通配符)
    fn matches_pattern(&self, url: &Url, pattern: &str) -> bool {
        // 简单实现:将 URL 转换为字符串后进行通配符匹配
        let url_str = url.as_str();

        // 使用 glob 模式匹配
        if let Ok(pat) = Pattern::new(pattern) {
            return pat.matches(url_str);
        }

        // 如果模式解析失败,尝试精确匹配
        url_str == pattern
    }

    /// 添加允许 URL 模式
    pub fn add_include(&mut self, pattern: String) {
        if !self.config.include.contains(&pattern) {
            self.config.include.push(pattern);
        }
    }

    /// 添加排除 URL 模式
    pub fn add_exclude(&mut self, pattern: String) {
        if !self.config.exclude.contains(&pattern) {
            self.config.exclude.push(pattern);
        }
    }

    /// 获取当前配置
    pub fn config(&self) -> &UrlSecurityConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_url() {
        let config = UrlSecurityConfig {
            include: vec!["https://api.example.com/*".to_string()],
            exclude: vec![],
        };
        let checker = UrlChecker::new(config);

        assert!(checker.is_safe("https://api.example.com/v1/users"));
    }

    #[test]
    fn test_wildcard_pattern() {
        let config = UrlSecurityConfig {
            include: vec!["http://localhost:*".to_string()],
            exclude: vec![],
        };
        let checker = UrlChecker::new(config);

        assert!(checker.is_safe("http://localhost:3000"));
        assert!(checker.is_safe("http://localhost:8080/api"));
    }

    #[test]
    fn test_excluded_url() {
        let config = UrlSecurityConfig {
            include: vec!["https://*".to_string()],
            exclude: vec!["https://blocked.com/*".to_string()],
        };
        let checker = UrlChecker::new(config);

        assert!(!checker.is_safe("https://blocked.com/secret"));
        assert!(checker.is_safe("https://allowed.com/page"));
    }

    #[test]
    fn test_invalid_url() {
        let config = UrlSecurityConfig::default();
        let checker = UrlChecker::new(config);

        assert!(!checker.is_safe("not a valid url"));
        assert!(!checker.is_safe(""));
    }

    #[test]
    fn test_not_allowed_url() {
        let config = UrlSecurityConfig {
            include: vec!["https://api.example.com/*".to_string()],
            exclude: vec![],
        };
        let checker = UrlChecker::new(config);

        assert!(!checker.is_safe("https://other.com/api"));
        assert!(!checker.is_safe("http://api.example.com"));
    }
}
