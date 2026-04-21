pub mod bus;
pub mod manager;
pub mod storage;
pub mod types;

// 便捷导出
pub use bus::MessageBus;
pub use manager::SessionManager;
pub use storage::{FileStorage, StorageBackend};
pub use types::{
    ActiveSpanInfo, Message, MessageError, MessageMeta, MessageType, Role, SessionState,
    Status,
};

#[cfg(test)]
pub mod test;