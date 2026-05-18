//! 中央ID生成器模块
//! 
//! 基于snowflaked算法实现统一的ID生成，提供session_id、span_id、request_id和task_id的生成
//! 所有ID格式统一为: {prefix}-{snowflake_id}

use std::sync::Arc;
use snowflaked::Generator;
use tracing;

/// ID生成器单例
static ID_GENERATOR: once_cell::sync::Lazy<Arc<std::sync::Mutex<Generator>>> = 
    once_cell::sync::Lazy::new(|| {
        Arc::new(std::sync::Mutex::new(Generator::new(1)))
    });

/// ID前缀常量
pub const SESSION_ID_PREFIX: &str = "S";
pub const REQUEST_ID_PREFIX: &str = "R";
pub const SPAN_ID_PREFIX: &str = "P";
pub const TASK_ID_PREFIX: &str = "T";

/// 生成Session ID (格式: R-{x})
pub fn generate_session_id() -> String {
    let id = generate_snowflake_id();
    let session_id = format!("{}-{}", SESSION_ID_PREFIX, id);
    
    tracing::debug!(
        session_id = %session_id,
        event = "session_id_generated"
    );
    
    session_id
}

/// 生成Request ID (格式: R-{x})
pub fn generate_request_id() -> String {
    let id = generate_snowflake_id();
    let request_id = format!("{}-{}", REQUEST_ID_PREFIX, id);
    
    tracing::debug!(
        request_id = %request_id,
        event = "request_id_generated"
    );
    
    request_id
}

/// 生成Span ID (格式: P-{x})
pub fn generate_span_id() -> String {
    let id = generate_snowflake_id();
    let span_id = format!("{}-{}", SPAN_ID_PREFIX, id);
    
    tracing::debug!(
        span_id = %span_id,
        event = "span_id_generated"
    );
    
    span_id
}

/// 生成Task ID (格式: T-{x})
pub fn generate_task_id() -> String {
    let id = generate_snowflake_id();
    let task_id = format!("{}-{}", TASK_ID_PREFIX, id);
    
    tracing::debug!(
        task_id = %task_id,
        event = "task_id_generated"
    );
    
    task_id
}

/// 内部方法：生成snowflake ID
fn generate_snowflake_id() -> u64 {
    let mut generator = ID_GENERATOR.lock().unwrap();
    generator.generate()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_id_format() {
        let session_id = generate_session_id();
        assert!(session_id.starts_with("S-"));
        assert!(!session_id.is_empty());
    }
    
    #[test]
    fn test_request_id_format() {
        let request_id = generate_request_id();
        assert!(request_id.starts_with("R-"));
        assert!(!request_id.is_empty());
    }
    
    #[test]
    fn test_span_id_format() {
        let span_id = generate_span_id();
        assert!(span_id.starts_with("P-"));
        assert!(!span_id.is_empty());
    }
    
    #[test]
    fn test_task_id_format() {
        let task_id = generate_task_id();
        assert!(task_id.starts_with("T-"));
        assert!(!task_id.is_empty());
    }
    
    #[test]
    fn test_id_uniqueness() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }
}
