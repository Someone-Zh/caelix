//! Caelix HTTP - HTTP API 后端
//!
//! 提供基于 HTTP/REST 的 API 服务

pub mod handlers;
pub mod server;

pub use server::start_http_server;
