//! 工具管理业务逻辑

use crate::types::ToolInfo;
use caelix_api::error::ApiError;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn list_tools(ctx: &CaelixContext) -> Result<Vec<ToolInfo>, ApiError> {
    let tools = ctx.tool_manager.list().await;
    let infos: Vec<ToolInfo> = tools
        .into_iter()
        .map(|tool| ToolInfo {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
        })
        .collect();
    Ok(infos)
}

pub(crate) async fn list_tool_names(ctx: &CaelixContext) -> Result<Vec<String>, ApiError> {
    Ok(ctx.tool_manager.list_names().await)
}

pub(crate) async fn get_tool_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<ToolInfo>, ApiError> {
    if let Some(tool) = ctx.tool_manager.get(name).await {
        Ok(Some(ToolInfo {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
        }))
    } else {
        Ok(None)
    }
}
