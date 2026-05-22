use std::path::{Path, PathBuf};
use crate::config::PathSecurityConfig;
use crate::checker::SecurityError;

/// 路径安全检测器
pub struct PathChecker {
    config: PathSecurityConfig,
}

impl PathChecker {
    /// 创建新的 PathChecker 实例
    pub fn new(config: PathSecurityConfig) -> Self {
        Self { config }
    }

    /// 检查路径是否可访问
    /// 
    /// 规则:
    /// 1. 如果路径在 exclude 列表中或其子目录,返回 false
    /// 2. 如果路径在 include 列表中或其子目录,返回 true
    /// 3. 否则返回 false
    pub fn is_safe(&self, path: &str) -> bool {
        let target_path = Path::new(path);
        
        // 首先检查是否在排除列表中
        for excluded in &self.config.exclude {
            if self.is_subpath(target_path, Path::new(excluded)) {
                return false;
            }
        }
        
        // 然后检查是否在允许列表中
        for included in &self.config.include {
            if self.is_subpath(target_path, Path::new(included)) {
                return true;
            }
        }
        
        false
    }

    /// 检查 target 是否是 base 的子路径或相同路径
    fn is_subpath(&self, target: &Path, base: &Path) -> bool {
        // 标准化路径后比较
        match (target.canonicalize(), base.canonicalize()) {
            (Ok(t), Ok(b)) => t.starts_with(&b),
            _ => {
                // 如果无法标准化,使用字符串前缀匹配
                let target_str = target.to_string_lossy();
                let base_str = base.to_string_lossy();
                target_str.starts_with(base_str.as_ref())
            }
        }
    }

    /// 添加允许路径
    pub fn add_include(&mut self, path: String) {
        if !self.config.include.contains(&path) {
            self.config.include.push(path);
        }
    }

    /// 添加排除路径
    pub fn add_exclude(&mut self, path: String) {
        if !self.config.exclude.contains(&path) {
            self.config.exclude.push(path);
        }
    }

    /// 获取当前配置
    pub fn config(&self) -> &PathSecurityConfig {
        &self.config
    }
}

/// 防止路径穿越攻击
pub fn sanitize_path(path: &str) -> Result<PathBuf, SecurityError> {
    let path_obj = Path::new(path);
    
    // 检查是否包含 ".." 组件
    for component in path_obj.components() {
        if component == std::path::Component::ParentDir {
            return Err(SecurityError::PathTraversalDetected);
        }
    }
    
    Ok(path_obj.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_allowed_path() {
        let config = PathSecurityConfig {
            include: vec!["/home/user".to_string()],
            exclude: vec![],
        };
        let checker = PathChecker::new(config);
        
        assert!(checker.is_safe("/home/user/project"));
        assert!(checker.is_safe("/home/user"));
    }
    
    #[test]
    fn test_excluded_path() {
        let config = PathSecurityConfig {
            include: vec!["/home/user".to_string()],
            exclude: vec!["/home/user/.git".to_string()],
        };
        let checker = PathChecker::new(config);
        
        assert!(!checker.is_safe("/home/user/.git"));
        assert!(!checker.is_safe("/home/user/.git/objects"));
    }
    
    #[test]
    fn test_not_allowed_path() {
        let config = PathSecurityConfig {
            include: vec!["/home/user".to_string()],
            exclude: vec![],
        };
        let checker = PathChecker::new(config);
        
        assert!(!checker.is_safe("/etc/passwd"));
        assert!(!checker.is_safe("/tmp/test"));
    }
    
    #[test]
    fn test_path_traversal() {
        assert!(sanitize_path("../etc/passwd").is_err());
        assert!(sanitize_path("/home/user/../secret").is_err());
        assert!(sanitize_path("/home/user/./file.txt").is_ok());
    }
}
