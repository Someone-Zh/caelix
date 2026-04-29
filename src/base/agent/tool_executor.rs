use crate::base::{AgentError, Tool,ToolCall};
use serde_json::Value;
use std::sync::Arc;

pub async fn execute_tool(
    tools: &[Arc<dyn Tool>],
    tool_call: &ToolCall,
) -> Result<(String, String), AgentError> {
    let tool_name = &tool_call.name;
    let raw_args = &tool_call.arguments;
    // 查找工具
    let tool = tools
        .iter()
        .find(|t| t.name() == tool_name)
        .ok_or_else(|| AgentError::ToolError(format!("工具不存在：{}", tool_name)))?;

    // 解析参数
    let clean_json_str =  raw_args.as_str().expect("args invalid");
    let args_json: Value = serde_json::from_str(clean_json_str).unwrap();

    let result = tool.execute(args_json).await;

    let result_str = match result.error {
        Some(err) => format!("工具执行错误：{}", err),
        None => result.output.to_string(),
    };
    Ok((tool.name().to_string(), result_str))
}