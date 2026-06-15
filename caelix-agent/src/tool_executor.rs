use caelix_api::AgentSpec;
use caelix_api::context::try_caelix_context;
use caelix_api::error::AgentError;
use caelix_api::tool::ToolResult;
use caelix_api::tool::{
    Tool, ToolApprovalType, ToolCall, ToolCallApprovalState, ToolPreCheckResult,
};
use std::sync::Arc;

/// 带预查（人工审批）的批量工具执行结果。
///
/// - `Executed`：所有 tool_calls 已执行（可能含已拒绝标记返回拒绝文本）
/// - `NeedApproval`：在执行到某条 tool_call 时需人工审批，中断执行；
///   已执行部分的 tool_result 已附在 `executed` 中，用于 loop_agent 优先写回
#[derive(Debug, Clone)]
pub enum ToolExecutionBatchResult {
    Executed(Vec<(String, String, String)>), // (tool_call_id, tool_name, output)
    NeedApproval {
        executed: Vec<(String, String, String)>,
        tool_call_id: String,
        tool_name: String,
        approval_type: caelix_api::tool::ToolApprovalType,
        parameters: serde_json::Value,
    },
}

/// 保留旧 API：执行一批 tool_calls，若有审批需要则当作错误（不推荐）。
/// 建议调用方改用 `execute_tools_static_with_pre_check` 以获得更精细的控制。
pub async fn execute_tools_static(
    def: &Arc<AgentSpec>,
    tool_calls: &[ToolCall],
) -> Result<Vec<(String, String, String)>, AgentError> {
    match execute_tools_static_with_pre_check(def, tool_calls).await {
        ToolExecutionBatchResult::Executed(results) => Ok(results),
        ToolExecutionBatchResult::NeedApproval { tool_name, .. } => Err(AgentError::ToolError(
            format!("工具【{}】需要人工审批", tool_name),
        )),
    }
}

/// 带预查的批量工具执行：
///
/// 对每个 tool_call：
/// 1. 若已被标记为 `Approved` → 直接执行
/// 2. 若已被标记为 `Rejected` → 生成一条拒绝文本作为 tool_result
/// 3. 否则先调用 `tool.pre_check(args)`：
///    - 返回 `None` → 直接执行
///    - 返回 `Some(pre_result)` → 立即中断，返回 `NeedApproval`
pub async fn execute_tools_static_with_pre_check(
    def: &Arc<AgentSpec>,
    tool_calls: &[ToolCall],
) -> ToolExecutionBatchResult {
    let mut executed = Vec::with_capacity(tool_calls.len());
    for tc in tool_calls {
        // 已拒绝：跳过执行，输出拒绝文本
        if tc.approval_state == Some(ToolCallApprovalState::Rejected) {
            executed.push((
                tc.id.clone(),
                tc.name.clone(),
                "[REJECTED] Tool execution was rejected by the user.".to_string(),
            ));
            continue;
        }

        // 未通过 pre_check 但 approval_state 不是 Approved 时，检查预查
        // 注意：approval_state == None 时也要走 pre_check
        let args_json = parse_arguments(&tc.arguments);

        // 先找工具
        let tool = match def.tools.iter().find(|t| t.name() == tc.name) {
            Some(t) => t,
            None => {
                executed.push((
                    tc.id.clone(),
                    tc.name.clone(),
                    format!("[ERROR] Tool '{}' not found", tc.name),
                ));
                continue;
            }
        };

        // approval_state 非 Approved 时，走预查
        if tc.approval_state != Some(ToolCallApprovalState::Approved) {
            for pre_result in tool.pre_checks(&args_json) {
                if !pre_check_allowed(&pre_result).await {
                    // 中断
                    return ToolExecutionBatchResult::NeedApproval {
                        executed,
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        approval_type: pre_result.approval_type,
                        parameters: pre_result.parameters,
                    };
                }
            }
        }

        // 已通过（或无需审批）：正常执行
        let result = tool.execute(args_json).await;
        let output = match result.error {
            Some(e) => format!("[ERROR] {}", e),
            None => result.output,
        };
        executed.push((tc.id.clone(), tc.name.clone(), output));
    }
    ToolExecutionBatchResult::Executed(executed)
}

async fn pre_check_allowed(pre_result: &ToolPreCheckResult) -> bool {
    let Some(ctx) = try_caelix_context() else {
        return false;
    };

    let security_checker = ctx.security_checker();
    match pre_result.approval_type {
        ToolApprovalType::Path => {
            let Some(path) = pre_result
                .parameters
                .get("path")
                .or_else(|| pre_result.parameters.get("file_path"))
                .and_then(|v| v.as_str())
            else {
                return false;
            };
            security_checker.check_path(path).await.is_ok()
        }
        ToolApprovalType::Url => {
            let Some(url) = pre_result.parameters.get("url").and_then(|v| v.as_str()) else {
                return false;
            };
            security_checker.check_url(url).await.is_ok()
        }
        ToolApprovalType::Command => {
            let Some(command) = pre_result
                .parameters
                .get("command")
                .and_then(|v| v.as_str())
            else {
                return false;
            };
            security_checker.check_command(command).await.is_ok()
        }
    }
}

/// 解析 tool_call.arguments 为 JsonValue。
///
/// tool_call.arguments 有两种形式：
/// - String 形式："{"a": 1}"
/// - 已解析的 Object：{"a": 1}
///
/// 这里做兼容处理。
fn parse_arguments(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                parsed
            } else {
                serde_json::Value::String(s.clone())
            }
        }
        other => other.clone(),
    }
}

pub async fn execute_tool(
    tools: &[Arc<dyn Tool>],
    tool_call: &ToolCall,
) -> Result<(String, ToolResult), AgentError> {
    let tool_name = &tool_call.name;
    let raw_args = &tool_call.arguments;
    // 查找工具
    let tool = tools
        .iter()
        .find(|t| t.name() == tool_name)
        .ok_or_else(|| AgentError::ToolError(format!("工具不存在：{}", tool_name)))?;

    let args_json = parse_arguments(raw_args);

    let result = tool.execute(args_json).await;
    Ok((tool.name().to_string(), result))
}
