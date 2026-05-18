//! Caelix Service - 服务层
//!
//! 提供 API trait 定义和实现，作为对外服务的统一接口层

pub mod api_trait;
pub mod api_impl;
pub mod types;

pub use api_trait::CaelixApi;
pub use api_impl::CaelixApiImpl;
pub use types::*;
