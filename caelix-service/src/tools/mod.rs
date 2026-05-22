pub mod list_tasks;
pub mod delegate_task;

pub use list_tasks::ListTasksTool;
pub use delegate_task::DelegateTaskTool;

use std::sync::Arc;
use caelix_api::tool::Tool;

/// 创建所有内置工具实例（统一管理所有工具的实例化）
pub fn create_all_builtin_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        // 文件编辑工具
        Arc::new(caelix_tools::DiffEditTool),
        // 目录树工具
        Arc::new(caelix_tools::DirectoryTreeTool),
        // 文件搜索工具
        Arc::new(caelix_tools::SmartSearchTool),
        // 文件读取工具
        Arc::new(caelix_tools::ReadFileTool),
    ]
}
