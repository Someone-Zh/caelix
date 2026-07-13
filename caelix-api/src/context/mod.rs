//! 上下文提供者接口抽象
//!
//! 定义轻量级的接口 trait，允许运行时层通过统一接口访问配置层的组件，
//! 避免 caelix-runtime 直接依赖 caelix-config
use crate::agent::AgentSpec;
use crate::commands::Command;
use crate::hooks::HookRegistry;
use crate::managers::{
    AgentManager, CommandManager, ProviderManager, Skill, SkillManager, ToolManager,
};
use crate::message::{MessageBusTrait, SessionManagerTrait};
use crate::plugins::PluginManager;
use crate::provider::{SessionUsageView, UsageRecord};
use crate::task::TaskManagerTrait;
use crate::utils::{generate_request_id, generate_session_id, generate_span_id, generate_trace_id};
use crate::variables::VariableManager;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct ProjectConfig {
    pub skills: HashMap<String, Arc<Skill>>,
    pub commands: Vec<Command>,
    pub agents: HashMap<String, Arc<AgentSpec>>,
}

#[async_trait]
pub trait ConfigOverlayTrait: Send + Sync {
    /// 确保指定工作目录的项目配置已加载（懒加载，仅首次或目录变更时加载）
    async fn ensure_project_config_loaded(&self, work_dir: &Path) -> Result<(), String>;
    /// 获取技能（项目优先，需先调用 ensure_project_config_loaded）
    async fn get_skill(&self, name: &str) -> Option<Arc<Skill>>;
    /// 获取指定工作目录的技能（项目优先，需先调用 ensure_project_config_loaded）
    async fn get_skill_for_work_dir(&self, work_dir: &Path, name: &str) -> Option<Arc<Skill>>;
    /// 获取命令（项目优先，需先调用 ensure_project_config_loaded）
    async fn get_command(&self, name: &str) -> Option<Command>;
    /// 获取指定工作目录的命令（项目优先，需先调用 ensure_project_config_loaded）
    async fn get_command_for_work_dir(&self, work_dir: &Path, name: &str) -> Option<Command>;
    /// 获取 AgentSpec（项目优先，需先调用 ensure_project_config_loaded；上层负责包装为 dyn Agent）
    async fn get_agent_spec(&self, name: &str) -> Option<Arc<AgentSpec>>;
    /// 获取指定工作目录的 AgentSpec（项目优先，需先调用 ensure_project_config_loaded）
    async fn get_agent_spec_for_work_dir(
        &self,
        work_dir: &Path,
        name: &str,
    ) -> Option<Arc<AgentSpec>>;
    /// 获取所有项目配置（只读）
    async fn project_configs(&self) -> tokio::sync::RwLockReadGuard<'_, HashMap<PathBuf, crate::context::ProjectConfig>>;
}

// ==================== 全局唯一 CaelixContext 存储 ====================

/// 全局存储 ContextProvider 的静态变量
static CAELIX_CONTEXT: OnceLock<Arc<dyn ContextProvider>> = OnceLock::new();

/// 设置全局 CaelixContext（程序启动时调用一次）
pub fn set_caelix_context(ctx: Arc<dyn ContextProvider>) {
    let _ = CAELIX_CONTEXT.set(ctx);
}

/// 获取全局 CaelixContext（如果已初始化）
pub fn caelix_context() -> Arc<dyn ContextProvider> {
    CAELIX_CONTEXT
        .get()
        .expect("CaelixContext 未初始化，请在程序启动时调用 set_caelix_context")
        .clone()
}

/// 安全地获取全局 CaelixContext（如果未初始化则返回 None）
pub fn try_caelix_context() -> Option<Arc<dyn ContextProvider>> {
    CAELIX_CONTEXT.get().cloned()
}

// ==================== EnvConfig Trait ====================

/// 环境配置 Trait
///
/// 暴露 CAELIX_HOME 路径与 debug 开关。
/// 具体实现位于 `caelix-config` 包中。
pub trait EnvConfigTrait: Send + Sync {
    /// 获取 CAELIX_HOME 目录路径
    fn caelix_home(&self) -> &Path;

    /// 是否启用 debug 模式
    fn debug_enabled(&self) -> bool;
}

impl std::fmt::Debug for dyn EnvConfigTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvConfigTrait")
            .field("caelix_home", &self.caelix_home())
            .field("debug_enabled", &self.debug_enabled())
            .finish()
    }
}

// ==================== SecurityChecker Trait ====================

/// 安全检查 Trait
///
/// 提供文件路径、URL 和命令的安全检查。
/// 具体实现位于 `caelix-security` 包中。
#[async_trait]
pub trait SecurityCheckerTrait: Send + Sync {
    /// 检查文件路径是否安全（不在黑名单中）
    async fn check_path(&self, path: &str) -> Result<(), String>;

    /// 检查 URL 是否安全（不在黑名单中）
    async fn check_url(&self, url: &str) -> Result<(), String>;

    /// 检查命令是否安全（在白名单中，且不在黑名单中）
    async fn check_command(&self, command: &str) -> Result<(), String>;
}

impl std::fmt::Debug for dyn SecurityCheckerTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityCheckerTrait").finish()
    }
}

// ==================== UsageTrackerTrait ====================

/// Token 用量追踪器 Trait
///
/// 允许上层（caelix-agent、caelix-service 等）在不知道具体实现的情况下
/// 记录与查询 Token 使用量。具体实现位于 `caelix-runtime` 中。
#[async_trait]
pub trait UsageTrackerTrait: Send + Sync {
    /// 记录一次 LLM 调用的用量
    async fn accumulate(&self, record: UsageRecord);

    /// 查询指定 session 的累计用量（含 context_size_tokens）
    async fn snapshot_session(
        &self,
        session_id: &str,
        ctx_window_tokens: Option<u32>,
    ) -> Option<SessionUsageView>;

    /// 查询全局用量（按 provider/model 维度汇总）
    async fn snapshot_global(&self) -> crate::provider::GlobalUsageView;
}

// ==================== AgentRunManagerTrait ====================

/// Agent 运行管理器 Trait
///
/// 管理正在运行的 Agent 任务，支持紧急停止。
/// 具体实现位于 `caelix-runtime` 包中。
#[async_trait]
pub trait AgentRunManagerTrait: Send + Sync {
    /// 停止指定 session 中正在运行的 Agent
    ///
    /// 返回 true 表示成功找到并触发停止，false 表示该 session 没有正在运行的 agent
    async fn stop_agent(&self, session_id: &str) -> bool;

    /// 获取指定 session 当前运行的取消令牌（子令牌）
    ///
    /// 若 session 当前无运行中的 Agent，返回 None。
    /// 返回的是子令牌，父令牌取消会级联到子令牌；子令牌取消不影响父令牌。
    fn get_cancel_token(&self, session_id: &str) -> Option<crate::cancel::CancellationToken>;
}

// ==================== ContextProvider Trait ====================

/// 统一的上下文入口 Trait
///
/// 通过这个 Trait，可以跨包访问 AgentManager、ToolManager、SessionManager、
/// MessageBus、TaskManager、SecurityChecker 等所有核心组件。
///
/// 具体实现位于 `caelix-runtime` 包中的 `CaelixContext`。
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn env_config(&self) -> &dyn EnvConfigTrait;
    fn agent_manager(&self) -> &AgentManager;
    fn tool_manager(&self) -> &ToolManager;
    fn llm_provider_manager(&self) -> &Arc<RwLock<ProviderManager>>;
    fn session_manager(&self) -> Arc<dyn SessionManagerTrait>;
    fn skill_manager(&self) -> &SkillManager;
    fn command_manager(&self) -> &CommandManager;
    fn hook_registry(&self) -> &HookRegistry;
    fn plugin_manager(&self) -> Arc<dyn PluginManager>;
    fn message_bus(&self) -> Arc<dyn MessageBusTrait>;
    fn task_manager(&self) -> Option<Arc<dyn TaskManagerTrait>>;
    fn security_checker(&self) -> Arc<dyn SecurityCheckerTrait>;
    fn variable_manager(&self) -> Arc<VariableManager>;
    /// 获取用量追踪器（若已初始化）
    fn usage_tracker(&self) -> Option<Arc<dyn UsageTrackerTrait>>;
    /// 获取 Agent 运行管理器（若已初始化）
    fn agent_run_manager(&self) -> Option<Arc<dyn AgentRunManagerTrait>>;
    /// 获取配置覆盖层（支持项目级配置覆盖全局配置）
    fn config_overlay(&self) -> Arc<dyn ConfigOverlayTrait>;
}

impl std::fmt::Debug for dyn ContextProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextProvider").finish()
    }
}

// ==================== Task Local 存储 ====================

tokio::task_local! {
    static CURRENT_CONTEXT: Arc<RuntimeContext>;
}

// 自定义Future扩展Trait，所有Future都能用.with_runtime_ctx
pub trait ContextFutureExt: Future + Sized {
    /// 绑定运行时上下文，future执行期间全局task_local可读取ctx
    fn with_runtime_ctx(
        self,
        ctx: Arc<RuntimeContext>,
    ) -> impl Future<Output = Self::Output> + Send + 'static
    where
        Self: Send + 'static,
        Self::Output: Send + 'static,
    {
        async move {
            // 在scope内执行future，作用域内所有子异步都能通过task_local获取ctx
            CURRENT_CONTEXT.scope(ctx.clone(), self).await
        }
    }
}
impl<F: Future> ContextFutureExt for F {}

/// Spawn 一个继承指定 RuntimeContext 的 Tokio 任务。
///
/// `tokio::task_local!` 不会自动跨 `tokio::spawn` 传播；所有需要在新任务中访问
/// `RuntimeContext::current/try_current` 的代码都应通过这个 helper 重新绑定上下文。
pub fn spawn_with_runtime_ctx<F>(
    ctx: Arc<RuntimeContext>,
    future: F,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move { CURRENT_CONTEXT.scope(ctx, future).await })
}

/// 运行时上下文 - Session 级别
///
/// 每个 Session 有独立的上下文实例，通过 tokio::task_local! 存储
/// 可以在任何异步代码中通过静态方法访问
pub struct RuntimeContext {
    /// Session ID - 标识一次完整的会话（多次请求）
    session_id: String,

    /// Request ID - 标识单次请求
    request_id: String,

    /// Span ID - 从 tracing span 自动提取，用于链路追踪
    span_id: String,

    /// Trace ID - 标识整个请求链路（多Agent协作时保持一致）
    trace_id: String,

    /// 工作目录 - Session 创建时设定，之后只读
    work_dir: PathBuf,

    /// Provider 名称 - 当前使用的 LLM 提供者（如 "openai", "bailian" 等）
    provider: String,

    /// Model 名称 - 当前使用的模型名称（如 "gpt-4", "qwen-max" 等）
    model: String,

    /// Debug 模式是否启用（协程内可覆盖全局设置）
    debug_enabled: bool,

    /// 取消令牌：用于紧急停止当前 Agent 执行
    cancellation_token: crate::cancel::CancellationToken,
}

impl std::fmt::Debug for RuntimeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeContext")
            .field("session_id", &self.session_id)
            .field("request_id", &self.request_id)
            .field("span_id", &self.span_id)
            .field("trace_id", &self.trace_id)
            .field("work_dir", &self.work_dir)
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("debug_enabled", &self.debug_enabled)
            .field("cancellation_token", &"CancellationToken")
            .field("context_provider", &"ContextProvider")
            .finish()
    }
}

impl Clone for RuntimeContext {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            request_id: self.request_id.clone(),
            span_id: self.span_id.clone(),
            trace_id: self.trace_id.clone(),
            work_dir: self.work_dir.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            debug_enabled: self.debug_enabled,
            cancellation_token: self.cancellation_token.clone(),
        }
    }
}

impl RuntimeContext {
    /// 创建新的 RuntimeContext
    ///
    /// # Arguments
    /// * `session_id` - Session ID，如果为空则自动生成
    /// * `request_id` - Request ID，如果为空则自动生成
    /// * `work_dir` - 工作目录
    /// * `provider` - Provider 名称（必填）
    /// * `model` - Model 名称（必填）
    /// * `debug_enabled` - Debug 模式是否启用
    /// * `cancellation_token` - 取消令牌，用于紧急停止
    pub fn new(
        session_id: Option<String>,
        request_id: Option<String>,
        work_dir: PathBuf,
        provider: String,
        model: String,
        debug_enabled: bool,
        cancellation_token: crate::cancel::CancellationToken,
    ) -> Self {
        let session_id = session_id.unwrap_or_else(generate_session_id);
        let request_id = request_id.unwrap_or_else(generate_request_id);
        let span_id = generate_span_id();
        let trace_id = generate_trace_id();

        Self {
            session_id,
            request_id,
            span_id,
            trace_id,
            work_dir,
            provider,
            model,
            debug_enabled,
            cancellation_token,
        }
    }

    /// 尝试获取当前上下文（安全版本）
    ///
    /// # Returns
    /// 如果在有效的 RuntimeContext 中，返回 Some(ctx)
    /// 否则返回 None
    pub fn try_current() -> Option<Arc<RuntimeContext>> {
        CURRENT_CONTEXT.try_with(|ctx| ctx.clone()).ok()
    }

    /// 获取当前 Session ID，如果不存在则使用提供的默认值
    pub fn current_or_default(&self) -> String {
        Self::try_current()
            .map(|ctx| ctx.session_id.clone())
            .unwrap_or_else(|| self.session_id.clone())
    }

    /// 获取当前 Provider，如果不存在则使用提供的默认值
    pub fn current_or_default_provider(&self) -> String {
        Self::try_current()
            .map(|ctx| ctx.provider.clone())
            .unwrap_or_else(|| self.provider.clone())
    }

    /// 获取当前 Model，如果不存在则使用提供的默认值
    pub fn current_or_default_model(&self) -> String {
        Self::try_current()
            .map(|ctx| ctx.model.clone())
            .unwrap_or_else(|| self.model.clone())
    }

    /// 获取 Session ID
    pub fn get_session_id(&self) -> &str {
        &self.session_id
    }

    /// 获取 Request ID
    pub fn get_request_id(&self) -> &str {
        &self.request_id
    }

    /// 获取 Span ID（从 tracing 自动提取）
    pub fn get_span_id(&self) -> &str {
        &self.span_id
    }

    /// 获取 Trace ID
    pub fn get_trace_id(&self) -> &str {
        &self.trace_id
    }

    /// 获取工作目录
    pub fn get_work_dir(&self) -> &PathBuf {
        &self.work_dir
    }

    /// 获取 Provider 名称
    pub fn get_provider(&self) -> &str {
        &self.provider
    }

    /// 获取 Model 名称
    pub fn get_model(&self) -> &str {
        &self.model
    }

    /// 获取取消令牌
    pub fn cancellation_token(&self) -> &crate::cancel::CancellationToken {
        &self.cancellation_token
    }

    /// 创建一个绑定了 session/request/trace ID 等字段的 `tracing::Span`。
    ///
    /// 进入这个 span 后，本 session 内的所有 `tracing::info!/debug!/warn!/error!`
    /// 等事件都会自动携带这些字段（前提是 subscriber 使用了 JSON 格式或
    /// 在字段表中包含 span 字段）。
    ///
    /// # 示例
    /// ```ignore
    /// let _guard = ctx.session_span().enter();
    /// tracing::info!("开始处理请求");
    /// ```
    pub fn session_span(&self) -> tracing::Span {
        tracing::info_span!(
            "session",
            session_id = %self.session_id,
            request_id = %self.request_id,
            span_id = %self.span_id,
            trace_id = %self.trace_id,
            provider = %self.provider,
            model = %self.model,
        )
    }
}

impl RuntimeContext {
    /// 获取当前运行时上下文
    ///
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic（类似 tokio::task_local 的行为）
    pub fn current() -> Arc<RuntimeContext> {
        CURRENT_CONTEXT.with(|ctx| ctx.clone())
    }
}
