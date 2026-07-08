use async_trait::async_trait;
use caelix_api::agent::AgentSpec;
use caelix_api::commands::Command;
use caelix_api::context::{
    AgentRunManagerTrait, ConfigOverlayTrait, ContextProvider, EnvConfigTrait,
    SecurityCheckerTrait, UsageTrackerTrait, set_caelix_context,
};
use caelix_api::managers::{
    AgentManager, CommandManager, ProviderManager, Skill, SkillManager, ToolManager,
};
use caelix_api::message::{MessageBusTrait, SessionManagerTrait};
use caelix_api::plugins::{PluginManager, PluginRegistry};
use caelix_api::task::TaskManagerTrait;
use caelix_api::variables::VariableManager;
use caelix_config::agents_loader::load_agents_from_directory;
use caelix_config::commands_loader::load_commands_from_directory;
use caelix_config::skills_loader::load_skills_from_directory;
use caelix_config::{AGENTS_DIR, COMMANDS_DIR, EnvConfig, SKILLS_DIR};
use caelix_message::{FileStorage, MessageBus, SessionManager};
use caelix_security::SecurityChecker;
use caelix_task::{FilePersistence, RunnableFactory, TaskManager};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::{AgentRunManager, HookRegistry, UsageTracker};

/// 单个工作目录的项目配置快照
#[derive(Default)]
struct ProjectConfig {
    skills: HashMap<String, Arc<Skill>>,
    commands: Vec<Command>,
    agents: HashMap<String, Arc<AgentSpec>>,
}

/// 配置覆盖层 - 支持项目级配置覆盖全局配置
///
/// 支持同时缓存多个工作目录的配置，按请求的 work_dir 懒加载。
#[derive(Clone)]
pub struct ConfigOverlay {
    global_skill_manager: Arc<SkillManager>,
    global_command_manager: Arc<CommandManager>,
    global_agent_manager: Arc<AgentManager>,
    global_tool_manager: Arc<ToolManager>,
    /// 多工作目录缓存：work_dir → 项目配置
    project_configs: Arc<RwLock<HashMap<PathBuf, ProjectConfig>>>,
    /// 每个工作目录独立的加载锁，避免并发首请求重复加载同一目录
    load_locks: Arc<DashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl ConfigOverlay {
    pub fn new(
        skill_manager: Arc<SkillManager>,
        command_manager: Arc<CommandManager>,
        agent_manager: Arc<AgentManager>,
        tool_manager: Arc<ToolManager>,
    ) -> Self {
        Self {
            global_skill_manager: skill_manager,
            global_command_manager: command_manager,
            global_agent_manager: agent_manager,
            global_tool_manager: tool_manager,
            project_configs: Arc::new(RwLock::new(HashMap::new())),
            load_locks: Arc::new(DashMap::new()),
        }
    }

    /// 懒加载：仅当指定 work_dir 下存在配置目录且尚未缓存时才加载
    async fn ensure_loaded(&self, work_dir: &Path) {
        // 快速路径：已缓存则跳过
        {
            let configs = self.project_configs.read().await;
            if configs.contains_key(work_dir) {
                return;
            }
        }

        let work_dir_buf = work_dir.to_path_buf();
        let load_lock = self
            .load_locks
            .entry(work_dir_buf.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _load_guard = load_lock.lock().await;

        {
            let configs = self.project_configs.read().await;
            if configs.contains_key(work_dir) {
                self.load_locks.remove(&work_dir_buf);
                return;
            }
        }

        // 检查工作目录下是否有任何配置目录
        let has_config = [SKILLS_DIR, COMMANDS_DIR, AGENTS_DIR]
            .iter()
            .any(|dir| work_dir.join(dir).exists());

        if !has_config {
            // 标记为已检查（空配置），避免反复检测目录
            self.project_configs
                .write()
                .await
                .insert(work_dir.to_path_buf(), ProjectConfig::default());
            self.load_locks.remove(&work_dir_buf);
            return;
        }

        let mut config = ProjectConfig::default();

        let skills_path = work_dir.join(SKILLS_DIR);
        if skills_path.exists() {
            match load_skills_from_directory(skills_path.to_str().unwrap_or("")).await {
                Ok(skills) => {
                    for skill in skills {
                        config
                            .skills
                            .insert(skill.full_name.clone(), Arc::new(skill));
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %skills_path.display(), error = %e, "Failed to load project skills")
                }
            }
        }

        let commands_path = work_dir.join(COMMANDS_DIR);
        if commands_path.exists() {
            match load_commands_from_directory(commands_path.to_str().unwrap_or("")).await {
                Ok(commands) => config.commands = commands,
                Err(e) => {
                    tracing::warn!(path = %commands_path.display(), error = %e, "Failed to load project commands")
                }
            }
        }

        let agents_path = work_dir.join(AGENTS_DIR);
        if agents_path.exists() {
            match load_agents_from_directory(
                agents_path.to_str().unwrap_or(""),
                &self.global_tool_manager,
            )
            .await
            {
                Ok(agent_specs) => {
                    for spec in agent_specs {
                        config.agents.insert(spec.name.clone(), Arc::new(spec));
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %agents_path.display(), error = %e, "Failed to load project agents")
                }
            }
        }

        tracing::info!(
            project_dir = %work_dir.display(),
            skills_count = config.skills.len(),
            commands_count = config.commands.len(),
            agents_count = config.agents.len(),
            "Project config loaded"
        );

        self.project_configs
            .write()
            .await
            .insert(work_dir.to_path_buf(), config);
        self.load_locks.remove(&work_dir_buf);
    }
}

#[async_trait]
impl ConfigOverlayTrait for ConfigOverlay {
    async fn ensure_project_config_loaded(&self, work_dir: &Path) -> Result<(), String> {
        self.ensure_loaded(work_dir).await;
        Ok(())
    }

    async fn get_skill(&self, name: &str) -> Option<Arc<Skill>> {
        // 遍历所有已缓存的项目配置查找（项目配置优先于全局）
        let configs = self.project_configs.read().await;
        for config in configs.values() {
            if let Some(skill) = config.skills.get(name) {
                return Some(skill.clone());
            }
        }
        drop(configs);
        self.global_skill_manager.get(name).await
    }

    async fn get_skill_for_work_dir(&self, work_dir: &Path, name: &str) -> Option<Arc<Skill>> {
        let configs = self.project_configs.read().await;
        if let Some(skill) = configs
            .get(work_dir)
            .and_then(|config| config.skills.get(name))
        {
            return Some(skill.clone());
        }
        drop(configs);
        self.global_skill_manager.get(name).await
    }

    async fn get_command(&self, name: &str) -> Option<Command> {
        let configs = self.project_configs.read().await;
        for config in configs.values() {
            if let Some(cmd) = config.commands.iter().find(|c| c.name == name) {
                return Some(cmd.clone());
            }
        }
        drop(configs);
        self.global_command_manager.get_by_name(name).await
    }

    async fn get_command_for_work_dir(&self, work_dir: &Path, name: &str) -> Option<Command> {
        let configs = self.project_configs.read().await;
        if let Some(cmd) = configs
            .get(work_dir)
            .and_then(|config| config.commands.iter().find(|c| c.name == name))
        {
            return Some(cmd.clone());
        }
        drop(configs);
        self.global_command_manager.get_by_name(name).await
    }

    async fn get_agent_spec(&self, name: &str) -> Option<Arc<AgentSpec>> {
        let configs = self.project_configs.read().await;
        for config in configs.values() {
            if let Some(spec) = config.agents.get(name) {
                return Some(spec.clone());
            }
        }
        drop(configs);
        // 全局层：AgentManager 存的是 dyn Agent，取其 spec
        let agent = self.global_agent_manager.get(name).await?;
        Some(agent.get_spec())
    }

    async fn get_agent_spec_for_work_dir(
        &self,
        work_dir: &Path,
        name: &str,
    ) -> Option<Arc<AgentSpec>> {
        let configs = self.project_configs.read().await;
        if let Some(spec) = configs
            .get(work_dir)
            .and_then(|config| config.agents.get(name))
        {
            return Some(spec.clone());
        }
        drop(configs);
        let agent = self.global_agent_manager.get(name).await?;
        Some(agent.get_spec())
    }
}

/// 项目上下文对象
/// 统一管理 AgentManager、ToolManager、ProviderManager 和 SessionManager 实例
#[derive(Clone)]
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
    /// 变量管理器实例
    pub variable_manager: Arc<VariableManager>,
    /// Token 用量追踪器
    pub usage_tracker: Arc<UsageTracker>,
    /// Agent 运行管理器（支持紧急停止）
    pub agent_run_manager: Arc<AgentRunManager>,
    /// 配置覆盖层（支持项目级配置覆盖全局配置）
    pub config_overlay: Arc<ConfigOverlay>,
}

impl std::fmt::Debug for CaelixContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaelixContext")
            .field("env_config", &self.env_config)
            .field("agent_manager", &self.agent_manager)
            .field("tool_manager", &self.tool_manager)
            .field("llm_provider_manager", &self.llm_provider_manager)
            .field("session_manager", &self.session_manager)
            .field("skill_manager", &self.skill_manager)
            .field("command_manager", &self.command_manager)
            .field("hook_registry", &self.hook_registry)
            .field("plugin_registry", &self.plugin_registry)
            .field("message_bus", &self.message_bus)
            .field("task_manager", &self.task_manager)
            .field("security_checker", &self.security_checker)
            .field("usage_tracker", &"UsageTracker")
            .field("agent_run_manager", &"AgentRunManager")
            .field("config_overlay", &"ConfigOverlay")
            .finish()
    }
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

        // 初始化 Token 用量追踪器（基于 caelix_home 目录）
        let usage_tracker = Arc::new(UsageTracker::new(&env_config.caelix_home));

        // 初始化 Agent 运行管理器
        let agent_run_manager = Arc::new(AgentRunManager::new());

        // 初始化管理器
        let skill_manager = Arc::new(SkillManager::new());
        let command_manager = Arc::new(CommandManager::new());
        let agent_manager = Arc::new(AgentManager::new());
        let tool_manager = Arc::new(ToolManager::new());

        // 初始化配置覆盖层
        let config_overlay = Arc::new(ConfigOverlay::new(
            skill_manager.clone(),
            command_manager.clone(),
            agent_manager.clone(),
            tool_manager.clone(),
        ));

        Self {
            env_config,
            agent_manager,
            tool_manager,
            llm_provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            session_manager,
            skill_manager,
            command_manager,
            hook_registry: Arc::new(HookRegistry::new()),
            plugin_registry: Arc::new(PluginRegistry::new()),
            message_bus: Arc::new(bus),
            task_manager: Some(task_manager),
            security_checker: Arc::new(SecurityChecker::new(
                caelix_security::SecurityConfig::default(),
            )),
            usage_tracker,
            agent_run_manager,
            config_overlay,
            variable_manager: Arc::new(VariableManager::new()),
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
        tracing::info!("Security checker initialized");

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
                    provider_manager.add_provider(named_provider.name, named_provider.provider)?;
                }
            }
        }

        // 4. 插件 → 技能管理器（必须在钩子之前加载，因为钩子会依赖技能）
        for plugin in self.plugin_registry.skill_plugins().await {
            for skill_def in plugin.skills().await? {
                let skill = Skill::from(skill_def);
                if let Err(e) = self.skill_manager.register(skill).await {
                    tracing::warn!(error = %e, "注册技能失败");
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
                    tracing::warn!(error = %e, "注册智能体失败");
                }
            }
        }

        // 7. 插件 → 命令管理器
        for plugin in self.plugin_registry.command_plugins().await {
            let commands = plugin.commands().await?;
            self.command_manager.register_batch(commands).await;
        }
        tracing::info!(
            commands_count = self.command_manager.get_all().await.len(),
            "Commands loaded"
        );

        // 8. 恢复持久化的任务
        if let Some(tm) = &self.task_manager {
            if let Err(e) = tm.restore().await {
                tracing::warn!(error = %e, "恢复任务失败");
            } else {
                tracing::info!("已恢复持久化的任务");
            }
        }

        // 9. 将自身注册为全局唯一上下文
        let ctx_arc: Arc<dyn ContextProvider> = Arc::new(self.clone());
        set_caelix_context(ctx_arc);
        tracing::info!("CaelixContext 已注册为全局变量");

        Ok(())
    }
}

// 实现 caelix-api 中定义的 ContextProvider trait
impl ContextProvider for CaelixContext {
    fn env_config(&self) -> &dyn EnvConfigTrait {
        &self.env_config
    }

    fn agent_manager(&self) -> &AgentManager {
        &self.agent_manager
    }

    fn tool_manager(&self) -> &ToolManager {
        &self.tool_manager
    }

    fn llm_provider_manager(&self) -> &Arc<RwLock<ProviderManager>> {
        &self.llm_provider_manager
    }

    fn session_manager(&self) -> Arc<dyn SessionManagerTrait> {
        self.session_manager.clone()
    }

    fn skill_manager(&self) -> &SkillManager {
        &self.skill_manager
    }

    fn command_manager(&self) -> &CommandManager {
        &self.command_manager
    }

    fn hook_registry(&self) -> &HookRegistry {
        &self.hook_registry
    }

    fn plugin_manager(&self) -> Arc<dyn PluginManager> {
        self.plugin_registry.clone()
    }

    fn message_bus(&self) -> Arc<dyn MessageBusTrait> {
        self.message_bus.clone()
    }

    fn task_manager(&self) -> Option<Arc<dyn TaskManagerTrait>> {
        self.task_manager
            .clone()
            .map(|tm| tm as Arc<dyn TaskManagerTrait>)
    }

    fn security_checker(&self) -> Arc<dyn SecurityCheckerTrait> {
        self.security_checker.clone()
    }

    fn variable_manager(&self) -> Arc<VariableManager> {
        self.variable_manager.clone()
    }

    fn usage_tracker(&self) -> Option<Arc<dyn UsageTrackerTrait>> {
        Some(self.usage_tracker.clone())
    }

    fn agent_run_manager(&self) -> Option<Arc<dyn AgentRunManagerTrait>> {
        Some(self.agent_run_manager.clone())
    }

    fn config_overlay(&self) -> Arc<dyn ConfigOverlayTrait> {
        self.config_overlay.clone()
    }
}

impl Default for CaelixContext {
    fn default() -> Self {
        Self::new()
    }
}
