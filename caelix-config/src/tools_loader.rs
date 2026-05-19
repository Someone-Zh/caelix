//! 工具加载器：自动注册所有项目内置工具到 ToolManager

use caelix_api::tool::Tool;
use std::sync::Arc;


/// 创建所有内置工具实例(统一管理所有工具的实例化)
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

/// 创建委派任务工具实例（无需参数，从 RuntimeContext 动态获取）
/// 注意：此函数现在返回 None，DelegateTaskTool 将在 CaelixContext::init_tools 中直接创建
pub fn create_delegate_task_tool() -> Option<Arc<dyn Tool>> {
    // DelegateTaskTool 需要在 CaelixContext 中创建，因为它依赖 caelix-service
    None
}
