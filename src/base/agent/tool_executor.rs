use crate::base::{AgentError, Tool};
use crate::base::agent::types::AgentOutputChunk;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

pub async fn execute_tool(
    tools: &[Arc<dyn Tool>],
    tool_name: String,
    raw_args: String,
    tx: &Sender<Result<AgentOutputChunk, AgentError>>,
) -> Result<(String, String), AgentError> {
    // 查找工具
    let tool = tools
        .iter()
        .find(|t| t.name() == tool_name)
        .ok_or_else(|| AgentError::ToolError(format!("工具不存在：{}", tool_name)))?;

    // 解析参数
    let clean_json_str = serde_json::from_str::<String>(&raw_args).unwrap_or(raw_args);
    let args_json: Value = serde_json::from_str(&clean_json_str).unwrap_or_else(|e| {
        eprintln!("工具参数解析失败: {} | 错误: {}", clean_json_str, e);
        serde_json::json!({})
    });

    let result = tool.execute(args_json).await;

    let result_str = match result.error {
        Some(err) => format!("工具执行错误：{}", err),
        None => result.output.to_string(),
    };

    // 返回工具结果
    let _ = tx.send(Ok(AgentOutputChunk::ToolResult {
        tool_name: tool.name().to_string(),
        result: result_str.clone(),
    })).await;

    Ok((tool.name().to_string(), result_str))
}