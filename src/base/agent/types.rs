use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::base::tool::Tool;

/// Agent 输出流分片
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentOutputChunk {
    Reasoning { content: String },
    Content { content: String },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        tool_name: String,
        result: String,
    },
    Finish { reason: String },
}

impl std::fmt::Display for AgentOutputChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentOutputChunk::Reasoning { content } => write!(f, "{}", content),
            AgentOutputChunk::Content { content } => write!(f, "{}", content),
            AgentOutputChunk::ToolCall { name, .. } => write!(f, "[工具调用: {}]", name),
            AgentOutputChunk::ToolResult { result, .. } => write!(f, "{}", result),
            AgentOutputChunk::Finish { .. } => write!(f, ""),
        }
    }
}

/// Agent 配置规格
#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub system_prompt: Arc<String>,  // 使用 Arc 避免字符串克隆
    pub tools: Vec<Arc<dyn Tool>>,   // 已经是 Arc，无需改变
    pub group: Option<Arc<String>>,  // 使用 Arc 避免 Option<String> 克隆
}