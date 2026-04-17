use crate::base::provider::*;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::base::AgentError;
use std::sync::Arc;

/// LLM提供者管理器模块
/// 对应架构：第一层 - 核心层
/// 该模块定义了LLM提供者的类型、配置和管理功能

/// LLM类型枚举
/// 定义了支持的LLM服务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmType {
    /// OpenAI服务
    OpenAI,
}

/// LLM提供者配置结构体
/// 定义了LLM提供者的配置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供者名称，用于在管理器中标识
    pub name: String,
    /// LLM服务类型
    pub llm_type: LlmType,
    /// API密钥，用于验证身份
    pub api_key: String,
    /// 基础URL，用于自定义API端点
    /// 为None时使用默认URL
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    /// 模型映射，将通用模型名称映射到具体服务的模型名称
    pub models: HashMap<String, String>,
    /// 额外选项，以JSON格式存储
    pub options: serde_json::Value,
}

/// LLM提供者管理器
/// 用于管理和访问不同的LLM提供者
#[derive(Debug)]
pub struct ProviderManager {
    /// 存储提供者的哈希映射，键为提供者名称
    providers: HashMap<String, Box<dyn LlmProvider>>,
    configs: HashMap<String, Arc<ProviderConfig>>,

}

impl ProviderManager {
    /// 创建新的LLM提供者管理器
    /// 
    /// # 返回值
    /// - `LlmProviderManager`: 新创建的管理器实例
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            configs: HashMap::new(),
        }
    }
    
    /// 添加新的LLM提供者
    /// 
    /// # 参数
    /// - `config`: 提供者的配置信息
    /// 
    /// # 返回值
    /// - `Result<(), AgentError>`: 操作结果
    pub fn add_provider(&mut self, config: ProviderConfig) -> Result<(), AgentError> {
        let name = config.name.clone();
        let api_key = config.api_key.clone();
        let base_url = config.base_url.clone();
        let provider: Box<dyn LlmProvider> = match config.llm_type {
            LlmType::OpenAI => {
                Box::new(OpenAIProvider::new(
                    api_key,
                    base_url,
                ))
            }
        };
        self.configs.insert(name.clone(), config.into());
        self.providers.insert(name, provider);
        Ok(())
    }
    
    /// 获取LLM提供者（不可变引用）
    /// 
    /// # 参数
    /// - `name`: 提供者名称
    /// 
    /// # 返回值
    /// - `Option<&Box<dyn LlmProvider>>`: 提供者的不可变引用，如不存在则为None
    pub fn get_provider(&self, name: &str) -> Option<&Box<dyn LlmProvider>> {
        self.providers.get(name)
    }
    pub fn get_config(&self, name: &str) -> Option<&Arc<ProviderConfig>> {
        self.configs.get(name)
    }
}