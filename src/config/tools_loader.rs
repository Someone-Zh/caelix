//! 工具加载器：自动注册所有项目内置工具到 ToolManager

use crate::base::tool::Tool;
use std::sync::Arc;


/// 创建所有内置工具实例（统一管理所有工具的实例化）
pub fn create_all_builtin_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        // 文件编辑工具
        Arc::new(crate::base::tool::DiffEditTool),
        // 目录树工具
        Arc::new(crate::base::tool::DirectoryTreeTool),
        // 文件搜索工具
        Arc::new(crate::base::tool::SmartSearchTool),
        // 文件读取工具
        Arc::new(crate::base::tool::ReadFileTool),
        // 任务列表工具
        Arc::new(crate::base::tool::ListTasksTool::new()),
    ]
}

/// 创建委派任务工具实例（无需参数，从 RuntimeContext 动态获取）
pub fn create_delegate_task_tool() -> Arc<dyn Tool> {
    Arc::new(crate::base::tool::DelegateTaskTool::new())
}
