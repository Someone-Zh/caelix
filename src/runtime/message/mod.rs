pub mod bus;
pub mod manager;
pub mod storage;
pub mod types;

// 便捷导出
pub use bus::MessageBus;
pub use manager::SessionManager;
#[allow(unused_imports)] // 公共API导出
pub use storage::{FileStorage, StorageBackend};
#[allow(unused_imports)] // 部分类型为将来扩展预留
pub use types::{
    ActiveSpanInfo, Message, MessageError, MessageMeta, MessageType, Role, SessionState,
    Status,
};
