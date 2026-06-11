pub mod loader;
pub mod message_bus_hook;
pub mod skill_hook;
pub mod tool_result_check_hook;

// 从 caelix-api 重新导出 HookRegistry 和相关类型
pub use caelix_api::hooks::*;
