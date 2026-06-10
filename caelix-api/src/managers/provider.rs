//! ProviderManager - LLM 提供者管理器

use std::collections::HashMap;
use std::sync::Arc;

use crate::provider::LlmProvider;

/// LLM 提供者管理器
#[derive(Debug)]
pub struct ProviderManager {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
}

impl ProviderManager {
    /// 创建新的 LLM 提供者管理器
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 添加新的 LLM 提供者
    pub fn add_provider(
        &mut self,
        name: String,
        provider: Arc<dyn LlmProvider>,
    ) -> Result<(), crate::error::AgentError> {
        self.providers.insert(name, provider);
        Ok(())
    }

    /// 获取 LLM 提供者（不可变引用）
    pub fn get_provider(&self, name: &str) -> Option<&Arc<dyn LlmProvider>> {
        self.providers.get(name)
    }

    /// 获取所有提供者
    pub fn get_all_providers(&self) -> Vec<(String, Arc<dyn LlmProvider>)> {
        self.providers
            .iter()
            .map(|(name, provider)| (name.clone(), provider.clone()))
            .collect()
    }
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}
