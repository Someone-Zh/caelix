//! Tool 核心定义模块
//!
//! 包含 Tool trait、ToolDefinition、ToolResult 等定义

use std::collections::HashMap;

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

impl ToolPreCheckResult {
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            approval_type: ToolApprovalType::Path,
            parameters: serde_json::json!({ "path": path.into() }),
        }
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self {
            approval_type: ToolApprovalType::Url,
            parameters: serde_json::json!({ "url": url.into() }),
        }
    }

    pub fn command(command: impl Into<String>) -> Self {
        Self {
            approval_type: ToolApprovalType::Command,
            parameters: serde_json::json!({ "command": command.into() }),
        }
    }
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

    /// 批量预查：用于一次工具调用同时涉及多种资源（如命令 + cwd）。
    fn pre_checks(&self, input: &JsonValue) -> Vec<ToolPreCheckResult> {
        self.pre_check(input).into_iter().collect()
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
        let arguments = match &self.arguments {
            serde_json::Value::String(s) => serde_json::Value::String(s.clone()),
            other => serde_json::Value::String(other.to_string()),
        };

        ApiToolCall {
            id: self.id.clone(),
            index: self.index,
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: self.name.clone(),
                arguments,
            },
        }
    }
}

#[derive(Debug, Default, Clone)]
struct ToolCallBuffer {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Default, Clone)]
pub struct ToolCallAggregator {
    buffers: HashMap<u32, ToolCallBuffer>,
    done: bool,
}

impl ToolCallAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn receive_delta(&mut self, delta: &ToolCall) {
        let index = delta.index;
        let args_delta = match &delta.arguments {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        if let Some(buf) = self.buffers.get_mut(&index) {
            buf.arguments.push_str(&args_delta);
            if !delta.id.is_empty() && buf.id.is_empty() {
                buf.id = delta.id.clone();
            }
            if !delta.name.is_empty() && buf.name.is_empty() {
                buf.name = delta.name.clone();
            }
        } else {
            let id = if delta.id.is_empty() {
                format!("call_{}", index)
            } else {
                delta.id.clone()
            };
            self.buffers.insert(
                index,
                ToolCallBuffer {
                    id,
                    name: delta.name.clone(),
                    arguments: args_delta,
                },
            );
        }
    }

    pub fn receive_deltas(&mut self, deltas: &[ToolCall]) {
        for d in deltas {
            self.receive_delta(d);
        }
    }

    pub fn mark_done(&mut self) {
        self.done = true;
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn snapshot(&self) -> Vec<ToolCall> {
        let mut entries: Vec<_> = self.buffers.iter().collect();
        entries.sort_by_key(|(idx, _)| **idx);
        entries
            .into_iter()
            .map(|(idx, buf)| ToolCall {
                id: buf.id.clone(),
                index: *idx,
                name: buf.name.clone(),
                arguments: serde_json::Value::String(buf.arguments.clone()),
                approval_state: None,
            })
            .collect()
    }

    pub fn completed_tool_calls(&self) -> Vec<ToolCall> {
        if !self.done {
            return Vec::new();
        }
        self.snapshot()
            .into_iter()
            .filter(|tc| {
                let args_str = match &tc.arguments {
                    serde_json::Value::String(s) => s.trim(),
                    other => return !other.is_null(),
                };
                if args_str.is_empty() {
                    return false;
                }
                serde_json::from_str::<serde_json::Value>(args_str).is_ok()
            })
            .map(|mut tc| {
                if let serde_json::Value::String(s) = &tc.arguments {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                        tc.arguments = parsed;
                    }
                }
                tc
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}
