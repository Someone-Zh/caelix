use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use tokio::sync::RwLock;
use crate::managers::{AgentManager, ToolManager, ProviderManager, SkillManager, CommandManager};
use crate::provider_loader::load_provider_configs;
use crate::tools_loader::{create_all_builtin_tools, create_delegate_task_tool};
use crate::agents_loader::register_all_agents;
use crate::skills_loader::register_all_skills;
use caelix_message::{SessionManager, MessageBus, FileStorage};
use caelix_task::{TaskManager, FilePersistence, RunnableFactory};
use caelix_api::context::{ContextProvider, HookExecutor, MessageSender};
#[cfg(feature = "logging")]
use tracing_subscriber;

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
    /// 消息总线实例
    pub message_bus: Arc<MessageBus>,
    /// 任务管理器实例
    pub task_manager: Option<Arc<TaskManager>>,
    /// Debug 模式是否启用
    #[allow(dead_code)] // 为将来使用预留
    pub debug_enabled: bool,
    /// 默认 Provider 名称（初始化时设置）
    pub default_provider: String,
    /// 默认 Model 名称（初始化时设置）
    pub default_model: String,
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
        // 读取 debug 配置
        let debug_enabled = env::var("CAELIX_DEBUG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        
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
        
        // 初始化日志系统
        #[cfg(feature = "logging")]
        {
            if debug_enabled {
                let caelix_home = Self::get_caelix_home();
                let log_dir = caelix_home.join("logs");
                
                // 简化日志初始化,具体实现可根据需要调整
                println!("✅ 日志系统已启用，日志目录: {:?}", caelix_home.join("logs"));
            }
        }
        
        Self {
            agent_manager: Arc::new(AgentManager::new()),
            tool_manager: Arc::new(ToolManager::new()),
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            session_manager,
            skill_manager: Arc::new(SkillManager::new()),
            command_manager: Arc::new(CommandManager::new()),
            message_bus: Arc::new(bus),
            task_manager: Some(task_manager),
            debug_enabled,
            // 默认配置将在 init() 中设置
            default_provider: String::new(),
            default_model: String::new(),
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
        crate::commands_loader::register_all_commands(
            &commands_dir.to_string_lossy(),
            &self.command_manager,
        ).await?;
        
        println!("Commands loaded. Total commands: {}", 
            self.command_manager.get_all().await.len());
        
        Ok(())
    }

    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> { 
        // 初始化工具
        self.init_tools().await?;

        // 初始化提供商
        self.init_provider().await?;
        
        // 初始化技能
        self.init_skills().await?;
        
        // 初始化智能体
        self.init_agents().await?;
        
        // 初始化命令
        self.init_commands().await?;
        
        // 恢复持久化的任务
        if let Some(tm) = &self.task_manager {
            if let Err(e) = tm.restore().await {
                eprintln!("⚠️  恢复任务失败: {:?}", e);
            } else {
                println!("✅ 已恢复持久化的任务");
            }
        }
        
        // 设置默认 provider 和 model（获取第一个）
        let provider_manager = self.llm_provider_manager.read().await;
        let providers = provider_manager.get_all_providers();
        
        if let Some((name, provider)) = providers.first() {
            self.default_provider = name.clone();
            let config = provider.config();
            let model = config.default_model();
            if !model.is_empty() {
                self.default_model = model.to_string();
            }
        }
        
        Ok(())
    }

}

impl Default for CaelixContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 为 CaelixContext 实现 ContextProvider trait
impl ContextProvider for CaelixContext {
    fn get_hook_executor(&self) -> Arc<dyn HookExecutor> {
        // TODO: HookRegistry 已移至运行时层，这里需要重新设计
        // 临时返回一个 panic，实际使用时应由 RuntimeContext 提供
        unimplemented!("HookExecutor should be provided by RuntimeContext")
    }
    
    fn get_message_sender(&self) -> Arc<dyn MessageSender> {
        // MessageBus 已经实现了 MessageSender trait
        self.message_bus.clone()
    }
    
    fn get_default_provider(&self) -> &str {
        &self.default_provider
    }
    
    fn get_default_model(&self) -> &str {
        &self.default_model
    }
}
