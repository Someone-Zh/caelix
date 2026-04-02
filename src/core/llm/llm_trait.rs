use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub id: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    pub content: Option<String>,
    pub id: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
}

// --- LLM 核心接口 ---
// 对应架构：第一层 - 核心层
#[async_trait]
pub trait LlmProvider: Send + Sync {
    // 流式对话接口，这对用户体验至关重要
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError>;
    
    // 非流式对话接口
    async fn chat(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<ChatResponse, AgentError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub model_name: String,
}