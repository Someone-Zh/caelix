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
            
            // 安全地检查 debug 模式，如果 context 不存在则跳过日志
            let should_log = std::panic::catch_unwind(|| RuntimeContext::is_debug_enabled())
                .unwrap_or(false);
            
            if should_log {
                if let Some(logger) = $crate::utils::logger::get_global_logger() {
                    // 安全地获取 context 信息，如果失败则使用默认值
                    let session_id = std::panic::catch_unwind(|| RuntimeContext::session_id())
                        .unwrap_or_else(|_| "unknown".to_string());
                    let request_id = std::panic::catch_unwind(|| RuntimeContext::request_id())
                        .unwrap_or_else(|_| "unknown".to_string());
                    let span_id = std::panic::catch_unwind(|| RuntimeContext::span_id())
                        .unwrap_or_else(|_| "unknown".to_string());
                    
                    logger.log(
                        $level,
                        &session_id,
                        &request_id,
                        &span_id,
                        $location,
                        $message,
                    );
                }
            }
        }
    };
}
