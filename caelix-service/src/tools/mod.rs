pub mod delegate_task;
pub mod list_tasks;

pub use delegate_task::DelegateTaskTool;
pub use list_tasks::ListTasksTool;

use caelix_api::tool::Tool;
use std::sync::Arc;

/// 创建所有内置工具实例（统一管理所有工具的实例化）
pub fn create_all_builtin_tools() -> Vec<Arc<dyn Tool>> {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(caelix_tools::DiffEditTool),
        Arc::new(caelix_tools::DirectoryTreeTool),
        Arc::new(caelix_tools::SmartSearchTool),
        Arc::new(caelix_tools::ReadFileTool),
        Arc::new(caelix_tools::StringReplaceTool),
        Arc::new(caelix_tools::CommandExecTool),
    ];

    #[cfg(feature = "ast")]
    {
        let mut tools = tools;
        tools.push(Arc::new(caelix_tools::ListSymbolsTool));
        tools.push(Arc::new(caelix_tools::GetSymbolDefinitionTool));
        tools
    }
    #[cfg(not(feature = "ast"))]
    tools
}
