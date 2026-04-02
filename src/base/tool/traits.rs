use async_trait::async_trait;
use crate::base::AgentError;
use serde_json::Value as JsonValue;

#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> JsonValue;
    async fn execute(&self, input: JsonValue) -> Result<JsonValue, AgentError>;

    // 克隆方法（必须手动实现，但我们有简化写法）
    fn clone_box(&self) -> Box<dyn Tool>;
}