//! 工具模块

#[cfg(feature = "logging")]
pub mod logger;

/// Debug 日志宏
/// 
/// 只有在 `logging` feature 启用时才会生成代码，否则展开为空
/// 
/// # 参数
/// - `$level`: 日志级别（"DEBUG", "INFO", "WARN", "ERROR"）
/// - `$session_id`: 会话 ID
/// - `$request_id`: 请求 ID
/// - `$span_id`: Span ID
/// - `$location`: 代码位置（通常使用 `format!("{}:{}", file!(), line!())`）
/// - `$message`: JSON 格式的消息内容（使用 `serde_json::json!` 宏）
/// 
/// # 示例
/// ```no_run
/// use caelix::debug_log;
/// use serde_json::json;
/// 
/// debug_log!(
///     "DEBUG",
///     "sess_123",
///     "req_456",
///     "span_789",
///     "main.rs:10",
///     json!({"event": "test", "value": 42})
/// );
/// ```
#[macro_export]
macro_rules! debug_log {
    ($level:expr, $session_id:expr, $request_id:expr, $span_id:expr, $location:expr, $message:expr) => {
        #[cfg(feature = "logging")]
        {
            if let Some(logger) = $crate::utils::logger::get_global_logger() {
                logger.log(
                    $level,
                    $session_id,
                    $request_id,
                    $span_id,
                    $location,
                    $message,
                );
            }
        }
    };
}

/// 便捷宏：从 RuntimeContext 自动获取追踪 ID
/// 
/// # 示例
/// ```no_run
/// use caelix::debug_log_ctx;
/// use serde_json::json;
/// 
/// debug_log_ctx!(
///     "DEBUG",
///     "main.rs:10",
///     json!({"event": "chat_start"})
/// );
/// ```
#[macro_export]
macro_rules! debug_log_ctx {
    ($level:expr, $location:expr, $message:expr) => {
        #[cfg(feature = "logging")]
        {
            use $crate::runtime::context::RuntimeContext;
            
            if RuntimeContext::is_debug_enabled() {
                if let Some(logger) = $crate::utils::logger::get_global_logger() {
                    logger.log(
                        $level,
                        &RuntimeContext::session_id(),
                        &RuntimeContext::request_id(),
                        &RuntimeContext::span_id(),
                        $location,
                        $message,
                    );
                }
            }
        }
    };
}
