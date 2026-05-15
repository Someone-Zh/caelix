//! Tool 核心模块
#![allow(dead_code)] // 部分API为将来扩展预留

pub mod traits;
pub mod file_edit;
pub mod tree;
pub mod file_search;
pub mod file_read;
pub mod delegate_task;
pub mod get_skill;
pub mod list_tasks;


pub use traits::{Tool, ToolCall,ToolDefinition,ToolResult};
pub use file_edit::DiffEditTool;
pub use tree::DirectoryTreeTool;
pub use file_search::SmartSearchTool;
pub use file_read::ReadFileTool;
pub use delegate_task::DelegateTaskTool;
pub use get_skill::GetSkillDetailTool;
pub use list_tasks::ListTasksTool;
pub use traits::ApiToolCall;
