use caelix_api::context::{ContextProvider, HookExecutor, MessageSender};
use caelix_api::managers::{
    AgentManager, CommandManager, ProviderManager, Skill, SkillManager, ToolManager,
};
use caelix_api::plugins::{PluginManager, PluginRegistry};
use caelix_config::EnvConfig;
use caelix_message::{FileStorage, MessageBus, SessionManager};
use caelix_security::SecurityChecker;
use caelix_task::{FilePersistence, RunnableFactory, TaskManager};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::HookRegistry;

/// 项目上下文对象
/// 统一管理 AgentManager、ToolManager、ProviderManager 和 SessionManager 实例
#[derive(Debug, Clone)]
pub struct CaelixContext {
    /// 环境变量配置
    pub env_config: EnvConfig,
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
    /// 插件注册中心实例
    pub plugin_registry: Arc<PluginRegistry>,
    /// 消息总线实例
    pub message_bus: Arc<MessageBus>,
    /// 任务管理器实例
    pub task_manager: Option<Arc<TaskManager>>,
    /// 安全检查器实例
    pub security_checker: Arc<SecurityChecker>,
    /// 默认 Provider 名称
    pub default_provider: String,
    /// 默认模型名称
    pub default_model: String,
}

impl CaelixContext {
    /// 创建新的应用上下文实例
    pub fn new() -> Self {
        let env_config = EnvConfig::new();

        // 初始化消息总线和存储
        let bus = MessageBus::new(1024);
        let storage = Arc::new(FileStorage::new("./sessions".to_string()));
        let session_manager = Arc::new(SessionManager::new(bus.clone(), storage));

        // 初始化任务管理器
        let task_persistence = Arc::new(FilePersistence::new("./tasks".to_string()));
        let runnable_factory = RunnableFactory::new();
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
            plugin_registry: Arc::new(PluginRegistry::new()),
            message_bus: Arc::new(bus),
            task_manager: Some(task_manager),
            security_checker: Arc::new(SecurityChecker::new(
                caelix_security::SecurityConfig::default(),
            )),
            default_provider: String::new(),
            default_model: String::new(),
        }
    }

    /// 注册插件到插件注册中心。
    pub async fn register_plugin(&self, plugin: Arc<dyn caelix_api::plugins::Plugin>) {
        self.plugin_registry.register_plugin(plugin).await;
    }

    /// 批量注册插件到插件注册中心。
    pub async fn register_plugins(&self, plugins: Vec<Arc<dyn caelix_api::plugins::Plugin>>) {
        self.plugin_registry.register_plugins(plugins).await;
    }

    /// 初始化整个应用上下文
    /// 插件是贡献物的唯一来源（Single Source of Truth）。
    /// 本方法只做两件事：1）从插件中读取贡献物并注册到各管理器；2）执行生命周期操作（安全配置、任务恢复、默认值）。
    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 加载安全配置
        let security_config =
            caelix_security::loader::load_security_config(&self.env_config.caelix_home)?;
        self.security_checker = Arc::new(SecurityChecker::new(security_config));
        println!("✅ Security checker initialized");

        // 2. 插件 → 工具管理器
        for plugin in self.plugin_registry.tool_plugins().await {
            for tool in plugin.tools().await? {
                self.tool_manager.register(tool).await;
            }
        }

        // 3. 插件 → LLM Provider 管理器
        {
            let mut provider_manager = self.llm_provider_manager.write().await;
            for plugin in self.plugin_registry.llm_provider_plugins().await {
                for named_provider in plugin.llm_providers().await? {
                    provider_manager
                        .add_provider(named_provider.name, named_provider.provider)?;
                }
            }
        }
        self.update_defaults().await;

        // 4. 插件 → 技能管理器（必须在钩子之前加载，因为钩子会依赖技能）
        for plugin in self.plugin_registry.skill_plugins().await {
            for skill_def in plugin.skills().await? {
                let skill = Skill::new(
                    skill_def.name,
                    skill_def.namespace,
                    skill_def.description,
                    skill_def.content,
                );
                if let Err(e) = self.skill_manager.register(skill).await {
                    eprintln!("⚠️  注册技能失败: {:?}", e);
                }
            }
        }

        // 5. 插件 → 钩子管理器（必须在 agents 之前，因为 agents 注册时会应用 init-hooks）
        for plugin in self.plugin_registry.hook_plugins().await {
            for hook in plugin.agent_hooks().await? {
                self.hook_registry.register_hook(hook).await;
            }
        }

        // 6. 插件 → 智能体管理器（插件内部已完成文件加载 + init-hooks 应用 + LoopAgent 包装）
        for plugin in self.plugin_registry.agent_plugins().await {
            for agent in plugin.agent_instances().await? {
                if let Err(e) = self.agent_manager.register(agent).await {
                    eprintln!("⚠️  注册智能体失败: {:?}", e);
                }
            }
        }

        // 7. 插件 → 命令管理器
        for plugin in self.plugin_registry.command_plugins().await {
            let commands = plugin.commands().await?;
            self.command_manager.register_batch(commands).await;
        }
        println!(
            "✅ Commands loaded. Total: {}",
            self.command_manager.get_all().await.len()
        );

        // 8. 恢复持久化的任务
        if let Some(tm) = &self.task_manager {
            if let Err(e) = tm.restore().await {
                eprintln!("⚠️  恢复任务失败: {:?}", e);
            } else {
                println!("✅ 已恢复持久化的任务");
            }
        }

        Ok(())
    }

    async fn update_defaults(&mut self) {
        let providers = self.llm_provider_manager.read().await.get_all_providers();
        if let Some((provider_name, provider)) = providers.first() {
            self.default_provider = provider_name.clone();
            self.default_model = provider.config().default_model().to_string();
        }
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
