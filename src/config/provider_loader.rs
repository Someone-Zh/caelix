use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use crate::base::provider::ProviderConfig;
/// Provider配置结

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

/// 加载提供商配置
pub fn load_provider_configs() -> Result<HashMap<String, ProviderConfig>, Box<dyn std::error::Error>> {
    let caelix_home = get_caelix_home();
    let provider_config_path = caelix_home.join("provider.json");
    
    // 如果目录不存在，创建它
    if !caelix_home.exists() {
        fs::create_dir_all(&caelix_home)?;
    }
    
    // 如果配置文件不存在，创建一个空的配置文件
    if !provider_config_path.exists() {
        let default_config: HashMap<String, ProviderConfig> = HashMap::new();
        let json_content = serde_json::to_string_pretty(&default_config)?;
        fs::write(&provider_config_path, json_content)?;
    }
    
    // 读取配置文件
    let content = fs::read_to_string(provider_config_path)?;
    let configs: HashMap<String, ProviderConfig> = serde_json::from_str(&content)?;
    
    Ok(configs)
}