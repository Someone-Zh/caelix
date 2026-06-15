//! 上下文提供者接口抽象
//!
//! 定义轻量级的接口 trait，允许运行时层通过统一接口访问配置层的组件，
//! 避免 caelix-runtime 直接依赖 caelix-config
use crate::hooks::HookRegistry;
use crate::managers::{AgentManager, CommandManager, ProviderManager, SkillManager, ToolManager};
use crate::message::{MessageBusTrait, SessionManagerTrait};
use crate::plugins::PluginManager;
use crate::task::TaskManagerTrait;
use crate::utils::{generate_request_id, generate_session_id, generate_span_id, generate_trace_id};
use async_trait::async_trait;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;

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
    /// * `context_provider` - 可选的上下文提供者
    pub fn new(
        session_id: Option<String>,
        request_id: Option<String>,
        work_dir: PathBuf,
        provider: String,
        model: String,
        debug_enabled: bool,
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
        }
    }

    /// 尝试获取当前上下文（安全版本）
    ///
    /// # Returns
    /// 如果在有效的 RuntimeContext 中，返回 Some(ctx)
    /// 否则返回 None
    pub fn try_current() -> Option<Arc<RuntimeContext>> {
        std::panic::catch_unwind(Self::current).ok()
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
