//! Runtime Context 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use crate::config::CaelixContext;

/// 运行时上下文 - Session 级别
/// 
/// 每个 Session 有独立的上下文实例，通过 tokio::task_local! 存储
/// 可以在任何异步代码中通过静态方法访问
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// Session ID - 标识一次完整的会话（多次请求）
    session_id: String,
    
    /// Request ID - 标识单次请求
    request_id: String,
    
    /// Span ID - 从 tracing span 自动提取，用于链路追踪
    span_id: String,
    
    /// 工作目录 - Session 创建时设定，之后只读
    work_dir: PathBuf,
    
    /// Provider 名称 - 当前使用的 LLM 提供者（如 "openai", "bailian" 等）
    provider: String,
    
    /// Model 名称 - 当前使用的模型名称（如 "gpt-4", "qwen-max" 等）
    model: String,
    
    /// Debug 模式是否启用（协程内可覆盖全局设置）
    debug_enabled: bool,
    
    /// 全局 CaelixContext 引用 - 所有 Session 共享
    caelix_context: Arc<CaelixContext>,
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
    /// * `caelix_context` - 全局上下文引用
    pub fn new(
        session_id: Option<String>,
        request_id: Option<String>,
        work_dir: PathBuf,
        provider: String,
        model: String,
        debug_enabled: bool,
        caelix_context: Arc<CaelixContext>,
    ) -> Self {
        let session_id = session_id.unwrap_or_else(|| format!("sess_{}", Uuid::new_v4()));
        let request_id = request_id.unwrap_or_else(|| format!("req_{}", Uuid::new_v4()));
        let span_id = Self::extract_span_id_from_tracing();
        
        Self {
            session_id,
            request_id,
            span_id,
            work_dir,
            provider,
            model,
            debug_enabled,
            caelix_context,
        }
    }
    
    /// 从当前 tracing span 提取 span_id
    /// 如果没有活跃的 span，则生成一个新的 UUID
    fn extract_span_id_from_tracing() -> String {
        // 尝试从 tracing 的当前 span 中提取 id
        if let Some(span) = tracing::Span::current().id() {
            format!("{:?}", span)
        } else {
            // 如果没有活跃 span，生成一个新的
            Uuid::new_v4().to_string()
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
    
    /// 获取全局 CaelixContext 引用
    pub fn get_caelix_context(&self) -> &Arc<CaelixContext> {
        &self.caelix_context
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
    
    /// 获取全局 CaelixContext
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn caelix_context() -> Arc<CaelixContext> {
        CURRENT_CONTEXT.with(|ctx| ctx.caelix_context.clone())
    }
    
    /// 获取当前 Debug 模式是否启用
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    pub fn is_debug_enabled() -> bool {
        CURRENT_CONTEXT.with(|ctx| ctx.debug_enabled)
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
    /// use std::sync::Arc;
    /// use caelix::config::CaelixContext;
    /// 
    /// #[tokio::main]
    /// async fn main() {
    ///     let caelix_ctx = Arc::new(CaelixContext::new());
    ///     let ctx = RuntimeContext::new(
    ///         None,
    ///         None,
    ///         std::env::current_dir().unwrap(),
    ///         "openai".to_string(),
    ///         "gpt-4".to_string(),
    ///         false,
    ///         caelix_ctx,
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
    
    /// 创建新的 Session 并进入其上下文
    /// 
    /// 这是一个便捷方法，用于快速创建和进入 Session 上下文
    /// 
    /// # Arguments
    /// * `session_id` - Session ID（可选，为空则自动生成）
    /// * `work_dir` - 工作目录
    /// * `provider` - Provider 名称（必填）
    /// * `model` - Model 名称（必填）
    /// * `caelix_context` - 全局上下文引用
    /// * `f` - 要执行的异步闭包
    pub async fn with_session<F, R>(
        session_id: Option<String>,
        work_dir: PathBuf,
        provider: String,
        model: String,
        caelix_context: Arc<CaelixContext>,
        f: F,
    ) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let context = RuntimeContext::new(session_id, None, work_dir, provider, model, false, caelix_context);
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
        let caelix_ctx = Arc::new(CaelixContext::new());
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("test_session".to_string()),
            Some("test_request".to_string()),
            work_dir.clone(),
            "openai".to_string(),
            "gpt-4".to_string(),
            false,
            caelix_ctx.clone(),
        );
        
        assert_eq!(ctx.get_session_id(), "test_session");
        assert_eq!(ctx.get_request_id(), "test_request");
        assert_eq!(ctx.get_work_dir(), &work_dir);
        assert_eq!(ctx.get_provider(), "openai");
        assert_eq!(ctx.get_model(), "gpt-4");
    }
    
    #[tokio::test]
    async fn test_context_scope() {
        let caelix_ctx = Arc::new(CaelixContext::new());
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("scope_test".to_string()),
            None,
            work_dir,
            "bailian".to_string(),
            "qwen-max".to_string(),
            false,
            caelix_ctx,
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
        let caelix_ctx = Arc::new(CaelixContext::new());
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            None,
            None,
            work_dir,
            "openai".to_string(),
            "gpt-4".to_string(),
            false,
            caelix_ctx,
        );
        
        // 验证自动生成的 ID 不为空
        assert!(!ctx.get_session_id().is_empty());
        assert!(!ctx.get_request_id().is_empty());
        assert!(!ctx.get_span_id().is_empty());
        
        // 验证 ID 格式
        assert!(ctx.get_session_id().starts_with("sess_"));
        assert!(ctx.get_request_id().starts_with("req_"));
        
        // 验证 provider 和 model
        assert_eq!(ctx.get_provider(), "openai");
        assert_eq!(ctx.get_model(), "gpt-4");
    }
}
