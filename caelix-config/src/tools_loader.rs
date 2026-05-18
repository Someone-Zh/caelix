//! 工具加载器：自动注册所有项目内置工具到 ToolManager

use caelix_api::tool::Tool;
use std::sync::Arc;


/// 创建所有内置工具实例(统一管理所有工具的实例化)
pub fn create_all_builtin_tools() -> Vec<Arc<dyn Tool>> {
    // TODO: ListTasksTool 需要从 src/base/tool 迁移到 caelix-task
    vec![
        // 文件编辑工具
        Arc::new(caelix_tools::DiffEditTool),
        // 目录树工具
        Arc::new(caelix_tools::DirectoryTreeTool),
        // 文件搜索工具
        Arc::new(caelix_tools::SmartSearchTool),
        // 文件读取工具
        Arc::new(caelix_tools::ReadFileTool),
        // 任务列表工具 - 暂时注释,等待迁移
        // Arc::new(crate::base::tool::ListTasksTool::new()),
    ]
}

/// 创建委派任务工具实例（无需参数，从 RuntimeContext 动态获取）
pub fn create_delegate_task_tool() -> Arc<dyn Tool> {
    // TODO: DelegateTaskTool 需要从 caelix-task 正确导出
    // 暂时返回一个占位实现或注释掉
    unimplemented!("DelegateTaskTool not yet migrated")
    // Arc::new(caelix_task::tools::DelegateTaskTool::new())
}
