pub mod types;
#[path = "trait.rs"]
pub mod trait_def;
pub mod core;

pub use trait_def::CaelixApi;
#[allow(unused_imports)] // 公共API导出
pub use types::ChatRequest;
pub use core::CaelixApiImpl;
