use crate::base::provider::*;
use std::collections::HashMap;
use crate::base::AgentError;
use std::sync::Arc;

/// LLM提供者管理器模块
/// 对应架构：第一层 - 核心层
/// 该模块定义了LLM提供者的类型、配置和管理功能



/// LLM提供者管理器
/// 用于管理和访问不同的LLM提供者
#[derive(Debug)]
pub struct ProviderManager {
    /// 存储提供者的哈希映射，键为提供者名称
    providers: HashMap<String, Arc<dyn LlmProvider>>,

}

impl ProviderManager {
    /// 创建新的LLM提供者管理器
    /// 
    /// # 返回值
    /// - `LlmProviderManager`: 新创建的管理器实例
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
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
        let provider: Arc<dyn LlmProvider> = match config.llm_type {
            LlmType::OpenAI => {
                Arc::new(OpenAIProvider::new(
                    config.into()
                ))
            }
        };
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
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LlmProvider>> {
        self.providers.get(name)
    }
    
    /// 获取所有提供者
    /// 
    /// # 返回值
    /// - `Vec<(String, Arc<dyn LlmProvider>)>`: 所有提供者的名称和实例
    pub fn get_all_providers(&self) -> Vec<(String, Arc<dyn LlmProvider>)> {
        self.providers.iter()
            .map(|(name, provider)| (name.clone(), provider.clone()))
            .collect()
    }
}