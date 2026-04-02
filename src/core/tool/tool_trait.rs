use async_trait::async_trait;
use super::*;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    // 返回 JSON Schema 定义，用于让 LLM 理解如何调用
    fn parameters_schema(&self) -> serde_json::Value;

    // 执行工具
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AgentError>;
}use async_trait::async_trait;
