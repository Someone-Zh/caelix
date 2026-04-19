use std::sync::Arc;
use tokio::sync::RwLock;
use crate::manager::AgentManager;
use crate::manager::ToolManager;
use crate::manager::ProviderManager;
use crate::config::provider_loader::load_provider_configs;
use crate::config::tools_loader::create_all_builtin_tools;
use crate::config::agents_loader::register_all_agents;
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
}

impl CaelixContext {
    /// 初始化提供商配置
    /// 读取配置文件并将提供商注册到 llm_provider_manager 中
    pub async fn init_provider(&self) -> Result<(), Box<dyn std::error::Error>> {
        let configs = load_provider_configs()?;
        
        let mut provider_manager = self.llm_provider_manager.write().await;
        for (key,mut config) in configs {
            if config.name.is_empty() {
                config.name = key
            }
            provider_manager.add_provider(config)?;
        }
        
        Ok(())
    }
    /// 初始化工具管理器
    /// 加载所有内置工具并注册到 tool_manager 中
    pub async fn init_tools(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 加载所有内置工具实例
        let tools = create_all_builtin_tools();

        // 获取工具管理器写锁
        let tool_manager = self.tool_manager.clone();
        // 批量注册工具
        for tool in tools {
            tool_manager.register(tool).await;
        }

        Ok(())
    }

    /// 初始化智能体管理器
    pub async fn init_agents(&self) -> Result<(), Box<dyn std::error::Error>> {
        register_all_agents(self).await?;
        Ok(())
    }

    pub async fn init(&self) -> Result<(), Box<dyn std::error::Error>> { 
        // 初始化工具
        self.init_tools().await?;

        // 初始化提供商
        self.init_provider().await?;

        // 初始化智能体
        self.init_agents().await?;
        Ok(())
    }

}

impl Default for CaelixContext {
    fn default() -> Self {
        Self::new()
    }
}