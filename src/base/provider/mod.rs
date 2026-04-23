//! LLM Provider 模块
#![allow(dead_code)] // 部分API为将来扩展预留

pub mod traits;
pub mod openai;

pub use traits::*;
pub use openai::*;