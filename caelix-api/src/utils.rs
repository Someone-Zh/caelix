//! 通用工具函数模块
//!
//! 包含ID生成器等公共工具

use std::sync::Arc;
use snowflaked::Generator;

/// ID生成器单例
static ID_GENERATOR: once_cell::sync::Lazy<Arc<std::sync::Mutex<Generator>>> = 
    once_cell::sync::Lazy::new(|| {
        Arc::new(std::sync::Mutex::new(Generator::new(1)))
    });

/// ID前缀常量
pub const SESSION_ID_PREFIX: &str = "S";
#[allow(dead_code)]
pub const REQUEST_ID_PREFIX: &str = "R";
#[allow(dead_code)]
pub const SPAN_ID_PREFIX: &str = "P";
pub const TASK_ID_PREFIX: &str = "T";
pub const TRACE_ID_PREFIX: &str = "E";

/// 生成Session ID (格式: S-{x})
pub fn generate_session_id() -> String {
    let id = generate_snowflake_id();
    format!("{}-{}", SESSION_ID_PREFIX, id)
}

/// 生成Request ID (格式: R-{x})
#[allow(dead_code)]
pub fn generate_request_id() -> String {
    let id = generate_snowflake_id();
    format!("{}-{}", REQUEST_ID_PREFIX, id)
}

/// 生成Span ID (格式: P-{x})
#[allow(dead_code)]
pub fn generate_span_id() -> String {
    let id = generate_snowflake_id();
    format!("{}-{}", SPAN_ID_PREFIX, id)
}

/// 生成Task ID (格式: T-{x})
pub fn generate_task_id() -> String {
    let id = generate_snowflake_id();
    format!("{}-{}", TASK_ID_PREFIX, id)
}

/// 生成 Trace ID (格式: E-{x})
pub fn generate_trace_id() -> String {
    let id = generate_snowflake_id();
    format!("{}-{}", TRACE_ID_PREFIX, id)
}

/// 内部方法：生成snowflake ID
fn generate_snowflake_id() -> u64 {
    let mut generator = ID_GENERATOR.lock().unwrap();
    generator.generate()
}
