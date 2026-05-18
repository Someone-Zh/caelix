//! Caelix HTTP - HTTP API 后端
//!
//! 提供基于 HTTP/REST 的 API 服务

pub mod server;
pub mod handlers;

pub use server::start_http_server;
