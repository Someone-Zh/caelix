//! Tool 核心模块
#![allow(dead_code)] // 部分API为将来扩展预留

pub mod traits;
pub mod file_edit;
pub mod tree;
pub mod file_search;
pub mod file_read;
pub mod get_skill;


pub use traits::{Tool, ToolCall,ToolDefinition,ToolResult};
pub use file_edit::DiffEditTool;
pub use tree::DirectoryTreeTool;
pub use file_search::SmartSearchTool;
pub use file_read::ReadFileTool;
pub use get_skill::GetSkillDetailTool;
pub use traits::ApiToolCall;

// 从 runtime 重新导出基础系统工具
pub use caelix_runtime::tools::ListTasksTool;
// 从 agent 重新导出 Agent 业务工具
pub use caelix_agent::tools::DelegateTaskTool;
