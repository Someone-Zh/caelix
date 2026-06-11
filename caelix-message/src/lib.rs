//! Caelix Message - 消息总线系统
//!
//! 包含 MessageBus、SessionManager、Storage 等
//!
//! 注意：消息类型定义已迁移到 caelix-api，本包仅保留实现

pub mod bus;
pub mod manager;
pub mod storage;
// agent_message、notification_message、task_message 模块仅保留兼容性导出
pub mod agent_message;
pub mod notification_message;
pub mod task_message;
pub mod types;

// 重新导出常用类型
pub use bus::MessageBus;
pub use manager::SessionManager;
pub use storage::FileStorage;
// 从 caelix-api 导入消息类型定义，确保全局统一
pub use caelix_api::message::{
    AgentMessage, AgentMessageType, NotificationMessage, NotificationType, TaskMessage,
    TaskMessageType,
};
pub use types::*;
