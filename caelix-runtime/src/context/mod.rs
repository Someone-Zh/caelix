use std::sync::Arc;
use tokio::sync::RwLock;
use caelix_api::managers::{AgentManager, ToolManager, ProviderManager, SkillManager, CommandManager};
use caelix_api::context::{ContextProvider, HookExecutor, MessageSender};
use caelix_message::{SessionManager, MessageBus};
use caelix_task::{TaskManager};
use caelix_api::hooks::HookRegistry;
use caelix_security::SecurityChecker;

/// 项目上下文对象
/// 统一管理 AgentManager、ToolManager、ProviderManager 和 SessionManager 实例
#[derive(Debug, Clone)]
pub struct CaelixContext {
    /// 环境变量配置
    pub env_config: caelix_config::EnvConfig,
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
    /// 安全检查器实例
    pub security_checker: Arc<SecurityChecker>,
}

impl CaelixContext {
    /// 创建新的应用上下文实例
    pub fn new() -> Self {
        // 初始化环境变量配置
        let env_config = caelix_config::EnvConfig::new();

        // 初始化消息总线和存储
        let bus = MessageBus::new(1024);
        let storage = Arc::new(caelix_message::FileStorage::new("./sessions".to_string()));
        let session_manager = Arc::new(SessionManager::new(bus.clone(), storage));

        // 初始化任务管理器
        let task_persistence = Arc::new(caelix_task::FilePersistence::new("./tasks".to_string()));
        let runnable_factory = caelix_task::RunnableFactory::new();
        let task_manager = Arc::new(TaskManager::new(
            Arc::new(bus.clone()),
            task_persistence,
            runnable_factory,
        ));

        Self {
            env_config,
            agent_manager: Arc::new(AgentManager::new()),
            tool_manager: Arc::new(ToolManager::new()),
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            session_manager,
            skill_manager: Arc::new(SkillManager::new()),
            command_manager: Arc::new(CommandManager::new()),
            hook_registry: Arc::new(HookRegistry::new()),
            message_bus: Arc::new(bus),
            task_manager: Some(task_manager),
            security_checker: Arc::new(SecurityChecker::new(caelix_security::SecurityConfig::default())),
        }
    }
}

impl CaelixContext {
    /// 初始化提供商配置
    pub async fn init_provider(&self) -> Result<(), Box<dyn std::error::Error>> {
        let caelix_home = &self.env_config.caelix_home;
        let _configs = caelix_config::provider_loader::load_provider_configs(caelix_home)?;
        Ok(())
    }

    /// 初始化工具管理器
    pub async fn init_tools(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// 初始化智能体管理器
    pub async fn init_agents(&self) -> Result<(), Box<dyn std::error::Error>> {
        let agents_dir = self.env_config.caelix_home.join("agents");

        if !agents_dir.exists() {
            std::fs::create_dir_all(&agents_dir)?;
            println!("Creating agents directory at: {:?}", agents_dir);
            println!("Please add .agent files to this directory");
        }

        // // 从 agents 目录加载并注册所有 agent
        // // 注意：这里注册的是实际的 Agent 实例（Arc<dyn Agent>）
        // // 使用 LoopAgent 作为默认实现
        // caelix_config::agents_loader::register_all_agents(
        //     self,
        //     &agents_dir.to_string_lossy(),
        //     |spec| {
        //         // 工厂函数：将 AgentSpec 包装为 LoopAgent 并转为 Arc<dyn Agent>
        //         Arc::new(caelix_agent::loop_agent::LoopAgent::new(spec))
        //     },
        // )
        // .await?;
        Ok(())
    }

    /// 初始化技能管理器
    pub async fn init_skills(&self) -> Result<(), Box<dyn std::error::Error>> {
        let skills_dir = self.env_config.caelix_home.join("skills");

        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir)?;
            println!("Creating skills directory at: {:?}", skills_dir);
        }

        caelix_config::skills_loader::register_all_skills(
            &skills_dir.to_string_lossy(),
            &self.skill_manager,
        )
        .await?;
        Ok(())
    }

    /// 初始化钩子系统
    pub async fn init_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// 初始化命令管理器
    pub async fn init_commands(&self) -> Result<(), Box<dyn std::error::Error>> {
        let commands_dir = self.env_config.caelix_home.join("commands");

        if !commands_dir.exists() {
            std::fs::create_dir_all(&commands_dir)?;
            println!("Creating commands directory at: {:?}", commands_dir);
        }

        caelix_config::commands_loader::register_all_commands(
            &commands_dir.to_string_lossy(),
            &self.command_manager,
        )
        .await?;

        println!(
            "Commands loaded. Total commands: {}",
            self.command_manager.get_all().await.len()
        );

        Ok(())
    }

    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 加载安全配置
        let security_config =
            caelix_security::loader::load_security_config(&self.env_config.caelix_home)?;

        // 重新创建 SecurityChecker
        self.security_checker = Arc::new(SecurityChecker::new(security_config));
        println!("✅ Security checker initialized");

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

        // 恢复持久化的任务
        if let Some(tm) = &self.task_manager {
            if let Err(e) = tm.restore().await {
                eprintln!("⚠️  恢复任务失败: {:?}", e);
            } else {
                println!("✅ 已恢复持久化的任务");
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

/// 为 CaelixContext 实现 HasAgentManager trait
impl caelix_config::agents_loader::HasAgentManager for CaelixContext {
    fn get_agent_manager(&self) -> Arc<AgentManager> {
        self.agent_manager.clone()
    }
}

/// 为 CaelixContext 实现 HasToolManager trait
impl caelix_config::agents_loader::HasToolManager for CaelixContext {
    fn get_tool_manager(&self) -> Arc<ToolManager> {
        self.tool_manager.clone()
    }
}

/// 为 CaelixContext 实现 ContextProvider trait
impl ContextProvider for CaelixContext {
    fn get_hook_executor(&self) -> Arc<dyn HookExecutor> {
        self.hook_registry.clone()
    }

    fn get_message_sender(&self) -> Arc<dyn MessageSender> {
        self.message_bus.clone()
    }
}
