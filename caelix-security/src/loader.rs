use crate::config::SecurityConfig;
use std::fs;
use std::path::Path;

/// 加载安全配置
pub fn load_security_config(
    caelix_home: &Path,
) -> Result<SecurityConfig, Box<dyn std::error::Error>> {
    let config_path = caelix_home.join("config.json");

    // 如果目录不存在,创建它
    if !caelix_home.exists() {
        fs::create_dir_all(caelix_home)?;
    }

    // 如果配置文件不存在,创建默认配置
    if !config_path.exists() {
        let default_config = SecurityConfig::default();
        let json_content = serde_json::to_string_pretty(&default_config)?;
        fs::write(&config_path, json_content)?;
        return Ok(default_config);
    }

    // 读取配置文件
    let content = fs::read_to_string(config_path)?;
    let config: SecurityConfig = serde_json::from_str(&content)?;

    Ok(config)
}

/// 保存安全配置到 config.json
pub fn save_security_config(
    caelix_home: &Path,
    config: &SecurityConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = caelix_home.join("config.json");

    // 确保目录存在
    if !caelix_home.exists() {
        fs::create_dir_all(caelix_home)?;
    }

    // 序列化并写入
    let json_content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, json_content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_default_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = load_security_config(temp_dir.path()).unwrap();

        assert!(config.path.include.is_empty());
        assert!(config.path.exclude.is_empty());
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();

        let mut config = SecurityConfig::default();
        config.path.include.push("/test/path".to_string());

        save_security_config(temp_dir.path(), &config).unwrap();

        let loaded = load_security_config(temp_dir.path()).unwrap();
        assert_eq!(loaded.path.include, vec!["/test/path"]);
    }
}
