use std::future::Future;
use crate::RuntimeContext;

impl RuntimeContext {
    pub fn with_ctx<F, R>(self, future: F) -> impl Future<Output = R> + Send + 'static
    where
        F: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let context = self;
        RuntimeContext::scope(context, future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_with_ctx() {
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("test_session".to_string()),
            None,
            work_dir,
            "test_provider".to_string(),
            "test_model".to_string(),
            false,
            None,
        );
        
        let result = ctx.with_ctx(async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_with_ctx_preserves_async_logic() {
        use std::time::Duration;
        
        let work_dir = std::env::current_dir().unwrap();
        
        let ctx = RuntimeContext::new(
            Some("async_test".to_string()),
            None,
            work_dir,
            "provider".to_string(),
            "model".to_string(),
            false,
            None,
        );
        
        let result = ctx.with_ctx(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let session = RuntimeContext::session_id();
            format!("Session: {}", session)
        }).await;
        
        assert_eq!(result, "Session: async_test");
    }
}
