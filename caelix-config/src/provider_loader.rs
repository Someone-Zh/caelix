#![allow(clippy::empty_line_after_doc_comments)]
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use caelix_api::provider::ProviderConfig;

/// Provider配置结

/// 加载提供商配置
pub fn load_provider_configs(caelix_home: &Path) -> Result<HashMap<String, ProviderConfig>, Box<dyn std::error::Error>> {
    let provider_config_path = caelix_home.join("provider.json");
    
    // 如果目录不存在，创建它
    if !caelix_home.exists() {
        fs::create_dir_all(caelix_home)?;
    }
    
    // 如果配置文件不存在，创建一个空的配置文件
    if !provider_config_path.exists() {
        let default_config: HashMap<String, ProviderConfig> = HashMap::new();
        let json_content = serde_json::to_string_pretty(&default_config)?;
        fs::write(&provider_config_path, json_content)?;
    }
    
    // 读取配置文件
    let content = fs::read_to_string(provider_config_path)?;
    let configs: HashMap<String, ProviderConfig> = match serde_json::from_str(&content) {
        Ok(configs) => configs,
        Err(e) =>{
            eprintln!("❌ 解析 provider.json 失败: {}", e);
            return Err(Box::new(e));
        }
    }; 
    
    Ok(configs)
}
