//! Tool 核心定义模块
//!
//! 包含 Tool trait、ToolDefinition、ToolResult 等定义

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub output: String,
    pub error: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> JsonValue;
    async fn execute(&self, input: JsonValue) -> ToolResult;

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
