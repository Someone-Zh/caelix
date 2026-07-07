//! Caelix Service - 服务层
//!
//! 提供 API trait 定义和实现，作为对外服务的统一接口层

pub mod api_impl;
pub mod api_trait;
pub mod plugins;
pub mod tools;
pub mod types;
pub mod variable_replacer;

pub use api_impl::CaelixApiImpl;
pub use api_trait::CaelixApi;
pub use tools::*;
pub use types::*;
pub use variable_replacer::*;
