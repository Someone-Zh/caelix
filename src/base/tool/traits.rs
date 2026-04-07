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

    // 克隆方法（必须手动实现，但我们有简化写法）
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

/// 工具调用结构体
/// 表示LLM请求调用外部工具的指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 工具调用的唯一标识符
    pub id: String,
    /// 要调用的工具名称
    pub name: String,
    /// 调用工具时传递的参数，以JSON格式表示
    pub arguments: serde_json::Value,
}