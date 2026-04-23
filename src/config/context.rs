use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use tokio::sync::RwLock;
use crate::manager::AgentManager;
use crate::manager::ToolManager;
use crate::manager::ProviderManager;
use crate::config::provider_loader::load_provider_configs;
use crate::config::tools_loader::{create_all_builtin_tools, create_delegate_task_tool};
use crate::config::agents_loader::register_all_agents;
use crate::runtime::message::{SessionManager, MessageBus, FileStorage};
/// 项目上下文对象
/// 统一管理 AgentManager、ToolManager、LlmProviderManager 和 SessionManager 实例
#[derive(Debug, Clone)]
pub struct CaelixContext {
    /// Agent 管理器实例
    pub agent_manager: Arc<AgentManager>,
    /// Tool 管理器实例
    pub tool_manager: Arc<ToolManager>,
    /// LLM 提供者管理器实例
    pub llm_provider_manager: Arc<RwLock<ProviderManager>>,
    /// 会话管理器实例
    pub session_manager: Arc<SessionManager>,
}

impl CaelixContext {
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

    /// 创建新的应用上下文实例
    pub fn new() -> Self {
        // 初始化消息总线和存储
        let bus = MessageBus::new(1024);
        let storage = Arc::new(FileStorage::new("./sessions".to_string()));
        let session_manager = Arc::new(SessionManager::new(bus, storage));
        
        Self {
            agent_manager: Arc::new(AgentManager::new()),
            tool_manager: Arc::new(ToolManager::new()),
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            session_manager,
        }
        
    }
}

impl CaelixContext {
    /// 初始化提供商配置
    /// 读取配置文件并将提供商注册到 llm_provider_manager 中
    pub async fn init_provider(&self) -> Result<(), Box<dyn std::error::Error>> {
        let caelix_home = Self::get_caelix_home();
        let configs = load_provider_configs(&caelix_home)?;
        
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

        // 注册委派任务工具（暂不配置 message_bus 和 task_manager）
        let delegate_tool = create_delegate_task_tool(Arc::new(self.clone()), None, None);
        tool_manager.register(delegate_tool).await;

        Ok(())
    }

    /// 初始化智能体管理器
    /// 从 CAELIX_HOME/agents 目录加载所有 .agent 文件
    pub async fn init_agents(&self) -> Result<(), Box<dyn std::error::Error>> {
        let caelix_home = Self::get_caelix_home();
        let agents_dir = caelix_home.join("agents");
        
        // 如果 agents 目录不存在，创建它并从嵌入的 conf 目录复制文件
        if !agents_dir.exists() {
            std::fs::create_dir_all(&agents_dir)?;
            println!("Creating agents directory at: {:?}", agents_dir);
            
            // 从嵌入的资源中复制 agent 文件
            use rust_embed::RustEmbed;
            
            #[derive(RustEmbed)]
            #[folder = "conf/agents/"]
            struct AgentAssets;
            
            for filename in AgentAssets::iter() {
                if let Some(asset) = AgentAssets::get(&filename) {
                    let file_path = agents_dir.join(filename.as_ref());
                    std::fs::write(&file_path, asset.data.as_ref())?;
                    println!("Copied agent file: {}", filename);
                }
            }
        }
        
        // 从 agents 目录加载并注册所有 agent
        register_all_agents(self, &agents_dir.to_string_lossy()).await?;
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