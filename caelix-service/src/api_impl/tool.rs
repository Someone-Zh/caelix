//! 工具管理业务逻辑

use crate::types::{ToolExecuteResult, ToolInfo};
use caelix_api::context::{ContextFutureExt, RuntimeContext};
use caelix_api::error::ApiError;
use caelix_runtime::context::CaelixContext;
use std::sync::Arc;

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

pub(crate) async fn execute_tool(
    ctx: &Arc<CaelixContext>,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolExecuteResult, ApiError> {
    let tool = ctx
        .tool_manager
        .get(tool_name)
        .await
        .ok_or_else(|| {
            ApiError::invalid_request(&format!("工具 '{}' 不存在", tool_name))
        })?;

    let cancel_token = caelix_api::cancel::CancellationToken::new();
    let work_dir = std::env::current_dir().unwrap_or_default();

    let runtime_ctx = Arc::new(RuntimeContext::new(
        None,
        None,
        work_dir,
        String::new(),
        String::new(),
        ctx.env_config.debug_enabled,
        cancel_token.clone(),
    ));

    let ctx_for_scope = runtime_ctx.clone();
    let tool_fut = async move { tool.execute(arguments).await };

    let cancel_fut = cancel_token.cancelled();
    let timeout_dur = std::time::Duration::from_secs(super::TOOL_EXECUTION_TIMEOUT_SECS);

    let result = tokio::select! {
        result = tool_fut.with_runtime_ctx(ctx_for_scope) => result,
        _ = cancel_fut => {
            return Ok(ToolExecuteResult {
                output: String::new(),
                error: Some("Tool execution cancelled".to_string()),
            });
        }
        _ = tokio::time::sleep(timeout_dur) => {
            return Ok(ToolExecuteResult {
                output: String::new(),
                error: Some(format!(
                    "Tool execution timed out ({}s)",
                    super::TOOL_EXECUTION_TIMEOUT_SECS
                )),
            });
        }
    };

    Ok(ToolExecuteResult {
        output: result.output,
        error: result.error,
    })
}
