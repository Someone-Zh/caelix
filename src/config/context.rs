use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use tokio::sync::RwLock;
use crate::manager::AgentManager;
use crate::manager::ToolManager;
use crate::manager::ProviderManager;
use crate::manager::SkillManager;
use crate::manager::CommandManager;
use crate::config::provider_loader::load_provider_configs;
use crate::config::tools_loader::{create_all_builtin_tools, create_delegate_task_tool};
use crate::config::agents_loader::register_all_agents;
use crate::config::skills_loader::register_all_skills;
use crate::enhancement::hooks::loader::HookLoader;
use crate::runtime::message::{SessionManager, MessageBus, FileStorage};
use crate::runtime::task::{TaskManager, FilePersistence, RunnableFactory};
use crate::enhancement::HookRegistry;
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
    /// 技能管理器实例
    pub skill_manager: Arc<SkillManager>,
    /// 命令管理器实例
    pub command_manager: Arc<CommandManager>,
    /// 钩子注册中心实例
    pub hook_registry: Arc<HookRegistry>,
    /// 消息总线实例
    pub message_bus: Arc<MessageBus>,
    /// 任务管理器实例
    pub task_manager: Option<Arc<TaskManager>>,
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
        let session_manager = Arc::new(SessionManager::new(bus.clone(), storage));
        
        // 初始化任务管理器
        let task_persistence = Arc::new(FilePersistence::new("./tasks".to_string()));
        let runnable_factory = RunnableFactory::new();
        // TODO: 在这里注册具体的 Runnable 构造函数
        let task_manager = Arc::new(TaskManager::new(
            Arc::new(bus.clone()),
            task_persistence,
            runnable_factory,
        ));
        
        Self {
            agent_manager: Arc::new(AgentManager::new()),
            tool_manager: Arc::new(ToolManager::new()),
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            session_manager,
            skill_manager: Arc::new(SkillManager::new()),
            command_manager: Arc::new(CommandManager::new()),
            hook_registry: Arc::new(HookRegistry::new()),
            message_bus: Arc::new(bus),
            task_manager: Some(task_manager),
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

        // 注册委派任务工具（无需参数，从 RuntimeContext 动态获取）
        let delegate_tool = create_delegate_task_tool();
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

    /// 初始化技能管理器
    /// 从 CAELIX_HOME/skills 目录加载所有 .skill 文件
    pub async fn init_skills(&self) -> Result<(), Box<dyn std::error::Error>> {
        let caelix_home = Self::get_caelix_home();
        let skills_dir = caelix_home.join("skills");
        
        // 如果 skills 目录不存在，创建它
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir)?;
            println!("Creating skills directory at: {:?}", skills_dir);
        }
        
        // 从 skills 目录加载并注册所有 skill
        register_all_skills(&skills_dir.to_string_lossy(), &self.skill_manager).await?;
        Ok(())
    }

    /// 初始化钩子系统
    /// 使用HookLoader注册所有内置钩子（如技能钩子）
    pub async fn init_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 使用HookLoader加载内置钩子
        HookLoader::load_builtin_hooks(
            &self.hook_registry,
            self.skill_manager.clone(),
        ).await?;
        
        Ok(())
    }

    /// 初始化命令管理器
    /// 从 CAELIX_HOME/commands 目录加载所有 .md 文件
    pub async fn init_commands(&self) -> Result<(), Box<dyn std::error::Error>> {
        let caelix_home = Self::get_caelix_home();
        let commands_dir = caelix_home.join("commands");
        
        // 如果 commands 目录不存在，创建它
        if !commands_dir.exists() {
            std::fs::create_dir_all(&commands_dir)?;
            println!("Creating commands directory at: {:?}", commands_dir);
        }
        
        // 从 commands 目录加载并注册所有命令
        crate::config::commands_loader::register_all_commands(
            &commands_dir.to_string_lossy(),
            &self.command_manager,
        ).await?;
        
        println!("Commands loaded. Total commands: {}", 
            self.command_manager.get_all().await.len());
        
        Ok(())
    }

    pub async fn init(&self) -> Result<(), Box<dyn std::error::Error>> { 
        // 初始化工具
        self.init_tools().await?;

        // 初始化提供商
        self.init_provider().await?;
        
        // 初始化技能（必须在钩子之前）
        self.init_skills().await?;
        
        // 初始化钩子（必须在agents之前，因为agents注册时需要应用init-hooks）
        self.init_hooks().await?;

        // 初始化智能体（会自动应用init-hooks进行增强）
        self.init_agents().await?;
        
        // 初始化命令
        self.init_commands().await?;
        
        Ok(())
    }

}

impl Default for CaelixContext {
    fn default() -> Self {
        Self::new()
    }
}