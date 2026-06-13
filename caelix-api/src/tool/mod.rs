//! Tool 核心定义模块
//!
//! 包含 Tool trait、ToolDefinition、ToolResult 等定义

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub output: String,
    pub error: Option<String>,
}

/// 审批类型：路径 / URL / 命令
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolApprovalType {
    Path,
    Url,
    Command,
}

/// 预查结果：若返回 Some 表示需要审批
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreCheckResult {
    pub approval_type: ToolApprovalType,
    pub parameters: JsonValue,
}

/// ToolCall 上的审批状态（业务侧使用，默认 None，持久化但不传 LLM）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCallApprovalState {
    Approved,
    Rejected,
}

#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> JsonValue;
    async fn execute(&self, input: JsonValue) -> ToolResult;

    /// 预查：返回是否需要审批，若需要则返回审批类型与参数。
    /// 默认 None 表示无需审批。各工具按需实现。
    fn pre_check(&self, _input: &JsonValue) -> Option<ToolPreCheckResult> {
        None
    }

    fn clone_box(&self) -> Box<dyn Tool>;

    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: self.parameters_schema(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(default)]
    pub index: u32,
    pub name: String,
    pub arguments: serde_json::Value,
    /// 审批状态：仅业务侧使用，默认 None（因此传给 LLM 时也不会包含）。
    /// 持久化到存储中会被保留，用于 resume 路径跳过预查。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub approval_state: Option<ToolCallApprovalState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCall {
    pub id: String,
    pub index: u32,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ApiToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCallFunction {
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ToolCall {
    pub fn to_api_format(&self) -> ApiToolCall {
        ApiToolCall {
            id: self.id.clone(),
            index: self.index,
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: self.name.clone(),
                arguments: self.arguments.clone(),
            },
        }
    }
}
