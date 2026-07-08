use crate::checker::SecurityError;
use crate::config::PathSecurityConfig;
use std::path::{Component, Path, PathBuf};

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
        let Ok(base) = base.canonicalize() else {
            return false;
        };
        let Ok(target) = canonicalize_existing_prefix(target) else {
            return false;
        };

        target.starts_with(base)
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

fn canonicalize_existing_prefix(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return path.canonicalize();
    }

    let mut missing_components = Vec::new();
    let mut existing = path;
    while !existing.exists() {
        let Some(parent) = existing.parent() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path has no existing ancestor",
            ));
        };

        if let Some(name) = existing.file_name() {
            missing_components.push(name.to_os_string());
        }
        existing = parent;
    }

    let mut canonical = existing.canonicalize()?;
    for component in missing_components.iter().rev() {
        let component_path = Path::new(component);
        if component_path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsafe path component",
            ));
        }
        canonical.push(component);
    }

    Ok(canonical)
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
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("user");
        let project = base.join("project");
        std::fs::create_dir_all(&project).unwrap();

        let config = PathSecurityConfig {
            include: vec![base.to_string_lossy().to_string()],
            exclude: vec![],
        };
        let checker = PathChecker::new(config);

        assert!(checker.is_safe(project.to_str().unwrap()));
        assert!(checker.is_safe(base.to_str().unwrap()));
    }

    #[test]
    fn test_excluded_path() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("user");
        let git = base.join(".git");
        std::fs::create_dir_all(git.join("objects")).unwrap();

        let config = PathSecurityConfig {
            include: vec![base.to_string_lossy().to_string()],
            exclude: vec![git.to_string_lossy().to_string()],
        };
        let checker = PathChecker::new(config);

        assert!(!checker.is_safe(git.to_str().unwrap()));
        assert!(!checker.is_safe(git.join("objects").to_str().unwrap()));
    }

    #[test]
    fn test_not_allowed_path() {
        let temp = tempfile::tempdir().unwrap();
        let allowed = temp.path().join("allowed");
        let denied = temp.path().join("denied");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&denied).unwrap();

        let config = PathSecurityConfig {
            include: vec![allowed.to_string_lossy().to_string()],
            exclude: vec![],
        };
        let checker = PathChecker::new(config);

        assert!(!checker.is_safe(denied.to_str().unwrap()));
    }

    #[test]
    fn test_path_traversal() {
        assert!(sanitize_path("../etc/passwd").is_err());
        assert!(sanitize_path("/home/user/../secret").is_err());
        assert!(sanitize_path("/home/user/./file.txt").is_ok());
    }

    #[test]
    fn test_prefix_collision_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("safe");
        let sibling = temp.path().join("safe-but-not-safe");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();

        let checker = PathChecker::new(PathSecurityConfig {
            include: vec![base.to_string_lossy().to_string()],
            exclude: vec![],
        });

        assert!(!checker.is_safe(sibling.join("file.txt").to_str().unwrap()));
    }

    #[test]
    fn test_new_file_under_allowed_existing_parent_is_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("safe");
        std::fs::create_dir_all(&base).unwrap();

        let checker = PathChecker::new(PathSecurityConfig {
            include: vec![base.to_string_lossy().to_string()],
            exclude: vec![],
        });

        assert!(checker.is_safe(base.join("new.txt").to_str().unwrap()));
    }
}
