pub mod types;
#[path = "trait.rs"]
pub mod trait_def;
pub mod core;

pub use trait_def::CaelixApi;
pub use types::ChatRequest;
pub use core::CaelixApiImpl;
