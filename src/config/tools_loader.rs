//! 工具加载器：自动注册所有项目内置工具到 ToolManager

use crate::base::tool::Tool;
use std::sync::Arc;
use crate::config::CaelixContext;
use std::sync::Arc as StdArc;


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
    ]
}

/// 创建委派任务工具实例（需要 context、message_bus 和 task_manager）
pub fn create_delegate_task_tool(
    context: StdArc<CaelixContext>,
    message_bus: Option<Arc<crate::runtime::MessageBus>>,
    task_manager: Option<Arc<crate::runtime::TaskManager>>,
) -> Arc<dyn Tool> {
    Arc::new(crate::base::tool::DelegateTaskTool::new(context, message_bus, task_manager))
}
