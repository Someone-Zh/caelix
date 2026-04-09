use std::sync::Arc;
use tokio::sync::RwLock;
use crate::base::AgentManager;
use crate::base::ToolManager;
use crate::base::ProviderManager;
use crate::config::provider_loader::load_provider_configs;

/// 项目上下文对象
/// 统一管理 AgentManager、ToolManager 和 LlmProviderManager 实例
#[derive(Debug, Clone)]
pub struct CaelixContext {
    /// Agent 管理器实例
    pub agent_manager: Arc<AgentManager>,
    /// Tool 管理器实例
    pub tool_manager: Arc<ToolManager>,
    /// LLM 提供者管理器实例
    pub llm_provider_manager: Arc<RwLock<ProviderManager>>,
}

impl CaelixContext {
    /// 创建新的应用上下文实例
    pub fn new() -> Self {
        Self {
            agent_manager: Arc::new(AgentManager::new()),
            tool_manager: Arc::new(ToolManager::new()),
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
        }
        
    }

    /// 获取默认的应用上下文实例
    pub fn default() -> Self {
        Self::new()
    }
}

impl CaelixContext {
    /// 初始化提供商配置
    /// 读取配置文件并将提供商注册到 llm_provider_manager 中
    pub async fn init_provider(&self) -> Result<(), Box<dyn std::error::Error>> {
        let configs = load_provider_configs()?;
        
        let mut provider_manager = self.llm_provider_manager.write().await;
        for (_, config) in configs {
            provider_manager.add_provider(config)?;
        }
        
        Ok(())
    }
}

impl Default for CaelixContext {
    fn default() -> Self {
        Self::new()
    }
}