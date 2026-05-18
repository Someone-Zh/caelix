pub mod agent_message;
pub mod notification_message;
pub mod task_message;
pub mod bus;
pub mod manager;
pub mod storage;
pub mod types;

// 便捷导出
#[allow(unused_imports)] // 公共API导出
pub use agent_message::{AgentMessage, AgentMessageType};
#[allow(unused_imports)] // 公共API导出
pub use notification_message::{NotificationMessage, NotificationType};
#[allow(unused_imports)] // 公共API导出
pub use task_message::{TaskMessage, TaskMessageType};
pub use bus::MessageBus;
pub use manager::SessionManager;
#[allow(unused_imports)] // 公共API导出
pub use storage::{FileStorage, StorageBackend};
#[allow(unused_imports)] // 部分类型为将来扩展预留
pub use types::{
    ActiveSpanInfo, Message, MessageError, MessageMeta, MessageType, Role, SessionState,
    Status,
};
