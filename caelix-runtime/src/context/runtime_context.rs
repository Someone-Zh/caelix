//! Runtime Context 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use std::path::PathBuf;
use std::sync::Arc;
use caelix_api::utils::{generate_session_id, generate_request_id, generate_span_id};
use caelix_api::context::ContextProvider;

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
    
    /// 可选的上下文提供者（用于访问上层组件如 hook_registry）
    context_provider: Option<Arc<dyn ContextProvider>>,
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
            .field("context_provider", &self.context_provider.as_ref().map(|_| "ContextProvider"))
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
            context_provider: self.context_provider.clone(),
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
        context_provider: Option<Arc<dyn ContextProvider>>,
    ) -> Self {
        let session_id = session_id.unwrap_or_else(generate_session_id);
        let request_id = request_id.unwrap_or_else(generate_request_id);
        let span_id = Self::extract_span_id_from_tracing();
        let trace_id = caelix_api::utils::generate_trace_id();
        
        Self {
            session_id,
            request_id,
            span_id,
            trace_id,
            work_dir,
            provider,
            model,
            debug_enabled,
            context_provider,
        }
    }

    /// 尝试获取当前上下文（安全版本）
    /// 
    /// # Returns
    /// 如果在有效的 RuntimeContext 中，返回 Some(ctx)
    /// 否则返回 None
    pub fn try_current() -> Option<RuntimeContext> {
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
    
    /// 从当前 tracing span 提取 span_id
    /// 如果没有活跃的 span，则生成一个新的 ID
    fn extract_span_id_from_tracing() -> String {
        // 尝试从 tracing 的当前 span 中提取 id
        if let Some(span) = tracing::Span::current().id() {
            format!("{:?}", span)
        } else {
            // 如果没有活跃 span，生成一个新的
            generate_span_id()
        }
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

// ==================== Task Local 存储 ====================

tokio::task_local! {
    static CURRENT_CONTEXT: RuntimeContext;
}

/// Session 守卫 - 确保上下文在作用域内有效
pub struct SessionGuard {
    _private: (),
}

impl SessionGuard {
    fn new() -> Self {
        Self {
            _private: (),
        }
    }
}

// ==================== 静态访问 API ====================

impl RuntimeContext {
    /// 获取当前运行时上下文
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic（类似 tokio::task_local 的行为）
    pub fn current() -> RuntimeContext {
        CURRENT_CONTEXT.with(|ctx| ctx.clone())
    }
    
    /// 获取当前 Session ID
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn session_id() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.session_id.clone())
    }
    
    /// 获取当前 Request ID
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn request_id() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.request_id.clone())
    }
    
    /// 获取当前 Span ID
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn span_id() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.span_id.clone())
    }
    
    /// 获取当前 Trace ID
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn trace_id() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.trace_id.clone())
    }
    
    /// 获取当前工作目录
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn work_dir() -> PathBuf {
        CURRENT_CONTEXT.with(|ctx| ctx.work_dir.clone())
    }
    
    /// 获取当前 Provider 名称
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn provider() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.provider.clone())
    }
    
    /// 获取当前 Model 名称
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn model() -> String {
        CURRENT_CONTEXT.with(|ctx| ctx.model.clone())
    }
    
    /// 获取当前 Debug 模式是否启用
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn is_debug_enabled() -> bool {
        CURRENT_CONTEXT.with(|ctx| ctx.debug_enabled)
    }
    
    /// 获取 ContextProvider（如果存在）
    pub fn get_context_provider(&self) -> Option<&Arc<dyn ContextProvider>> {
        self.context_provider.as_ref()
    }
    
    /// 获取 HookExecutor（通过 ContextProvider）
    pub fn get_hook_executor(&self) -> Option<Arc<dyn caelix_api::context::HookExecutor>> {
        self.context_provider.as_ref().map(|p| p.get_hook_executor())
    }
    
    /// 获取 MessageSender（通过 ContextProvider）
    pub fn get_message_sender(&self) -> Option<Arc<dyn caelix_api::context::MessageSender>> {
        self.context_provider.as_ref().map(|p| p.get_message_sender())
    }
    
    /// 便捷方法：获取默认 Provider
    pub fn default_provider(&self) -> Option<String> {
        self.context_provider.as_ref().map(|p| p.get_default_provider().to_string())
    }
    
    /// 便捷方法：获取默认 Model
    pub fn default_model(&self) -> Option<String> {
        self.context_provider.as_ref().map(|p| p.get_default_model().to_string())
    }
    
    /// 在指定的上下文中执行异步闭包
    /// 
    /// # Arguments
    /// * `context` - 运行时上下文
    /// * `f` - 要执行的异步闭包
    /// 
    /// # Example
    /// ```no_run
    /// use caelix::runtime::context::RuntimeContext;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let ctx = RuntimeContext::new(
    ///         None,
    ///         None,
    ///         std::env::current_dir().unwrap(),
    ///         "openai".to_string(),
    ///         "gpt-4".to_string(),
    ///         false,
    ///     );
    ///     
    ///     RuntimeContext::scope(ctx, async {
    ///         let session_id = RuntimeContext::session_id();
    ///         println!("Current session: {}", session_id);
    ///     }).await;
    /// }
    /// ```
    pub async fn scope<F, R>(context: RuntimeContext, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        CURRENT_CONTEXT.scope(context, f).await
    }
    
    /// 同步版本的 scope（用于非异步代码）
    /// 
    /// # Arguments
    /// * `context` - 运行时上下文
    /// * `f` - 要执行的闭包
    pub fn scope_sync<F, R>(context: RuntimeContext, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        CURRENT_CONTEXT.sync_scope(context, f)
    }
    
    /// 在指定上下文中 spawn 异步任务
    /// 
    /// 这是一个便捷方法，确保在 tokio::spawn 中正确传递 RuntimeContext
    /// 
    /// # Arguments
    /// * `context` - 运行时上下文
    /// * `future` - 要执行的异步任务
    /// 
    /// # Returns
    /// tokio::task::JoinHandle
    /// 
    /// # Example
    /// ```no_run
    /// use caelix::runtime::context::RuntimeContext;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let ctx = RuntimeContext::new(
    ///         None,
    ///         None,
    ///         std::env::current_dir().unwrap(),
    ///         "openai".to_string(),
    ///         "gpt-4".to_string(),
    ///         false,
    ///     );
    ///     
    ///     let handle = RuntimeContext::spawn_with_context(ctx, async {
    ///         let session_id = RuntimeContext::session_id();
    ///         println!("Current session: {}", session_id);
    ///     });
    ///     handle.await.unwrap();
    /// }
    /// ```
    pub fn spawn_with_context<F>(context: RuntimeContext, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(async move {
            CURRENT_CONTEXT.scope(context, future).await
        })
    }
    
    /// 创建新的 Session 并进入其上下文
    /// 
    /// 这是一个便捷方法，用于快速创建和进入 Session 上下文
    /// 
    /// # Arguments
    /// * `session_id` - Session ID（可选，为空则自动生成）
    /// * `work_dir` - 工作目录
    /// * `provider` - Provider 名称（必填）
    /// * `model` - Model 名称（必填）
    /// * `model` - Model 名称（必填）
    /// * `f` - 要执行的异步闭包
    pub async fn with_session<F, R>(
        session_id: Option<String>,
        work_dir: PathBuf,
        provider: String,
        model: String,
        f: F,
    ) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let context = RuntimeContext::new(session_id, None, work_dir, provider, model, false, None);
        CURRENT_CONTEXT.scope(context, f).await
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_context_creation() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("test_session".to_string()),
            Some("test_request".to_string()),
            work_dir.clone(),
            "openai".to_string(),
            "gpt-4".to_string(),
            false,
            None,
        );
        
        assert_eq!(ctx.get_session_id(), "test_session");
        assert_eq!(ctx.get_request_id(), "test_request");
        assert_eq!(ctx.get_work_dir(), &work_dir);
        assert_eq!(ctx.get_provider(), "openai");
        assert_eq!(ctx.get_model(), "gpt-4");
    }
    
    #[tokio::test]
    async fn test_context_scope() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("scope_test".to_string()),
            None,
            work_dir,
            "bailian".to_string(),
            "qwen-max".to_string(),
            false,
            None,
        );
        
        let result = RuntimeContext::scope(ctx, async {
            let session_id = RuntimeContext::session_id();
            assert_eq!(session_id, "scope_test");
            true
        }).await;
        
        assert!(result);
    }
    
    #[tokio::test]
    async fn test_auto_generate_ids() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            None,
            None,
            work_dir,
            "openai".to_string(),
            "gpt-4".to_string(),
            false,
            None,
        );
        
        // 验证自动生成的 ID 不为空
        assert!(!ctx.get_session_id().is_empty());
        assert!(!ctx.get_request_id().is_empty());
        assert!(!ctx.get_span_id().is_empty());
        assert!(!ctx.get_trace_id().is_empty());
        
        // 验证 ID 格式
        assert!(ctx.get_session_id().starts_with("S-"));
        assert!(ctx.get_request_id().starts_with("R-"));
        assert!(ctx.get_trace_id().starts_with("E-"));
        
        // 验证 provider 和 model
        assert_eq!(ctx.get_provider(), "openai");
        assert_eq!(ctx.get_model(), "gpt-4");
    }
    
    #[tokio::test]
    async fn test_try_current_without_context() {
        let result = RuntimeContext::try_current();
        assert!(result.is_none(), "Expected None when no context is set");
    }
    
    #[tokio::test]
    async fn test_try_current_with_context() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("try_current_test".to_string()),
            None,
            work_dir,
            "test_provider".to_string(),
            "test_model".to_string(),
            false,
            None,
        );
        
        let result = RuntimeContext::scope(ctx, async {
            RuntimeContext::try_current()
        }).await;
        
        assert!(result.is_some(), "Expected Some when context is set");
        assert_eq!(result.unwrap().get_session_id(), "try_current_test");
    }
    
    #[tokio::test]
    async fn test_current_or_default_with_context() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("context_session".to_string()),
            None,
            work_dir.clone(),
            "context_provider".to_string(),
            "context_model".to_string(),
            false,
            None,
        );
        
        let default_session = "default_session".to_string();
        let result = RuntimeContext::scope(ctx, async move {
            RuntimeContext::try_current()
                .map(|c| c.session_id.clone())
                .unwrap_or_else(|| default_session.clone())
        }).await;
        
        assert_eq!(result, "context_session");
    }
    
    #[tokio::test]
    async fn test_current_or_default_provider() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("test".to_string()),
            None,
            work_dir,
            "my_provider".to_string(),
            "my_model".to_string(),
            false,
            None,
        );
        
        let default_provider = "default_provider".to_string();
        let result = RuntimeContext::scope(ctx, async move {
            RuntimeContext::try_current()
                .map(|c| c.provider.clone())
                .unwrap_or_else(|| default_provider.clone())
        }).await;
        
        assert_eq!(result, "my_provider");
    }
    
    #[tokio::test]
    async fn test_runtime_context_snapshot_try_from_current() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("snapshot_test".to_string()),
            None,
            work_dir,
            "snapshot_provider".to_string(),
            "snapshot_model".to_string(),
            false,
            None,
        );
        
        let result = RuntimeContext::scope(ctx, async {
            RuntimeContext::try_current()
        }).await;
        
        assert!(result.is_some());
        let ctx = result.unwrap();
        assert_eq!(ctx.provider.clone(), "snapshot_provider");
        assert_eq!(ctx.model.clone(), "snapshot_model");
    }
}
