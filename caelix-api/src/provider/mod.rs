//! LLM Provider 核心定义模块
//!
//! 包含 LlmProvider trait、ChatMessage、TokenUsage、LlmConfig 等定义

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;

use crate::error::AgentError;
use crate::tool::{ToolCall, ToolDefinition};

/// Token 用量信息（来自模型响应末尾的 usage 字段）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Claude / DeepSeek 等模型的 reasoning token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// OpenAI prompt_cache_details 中的缓存命中 token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_tokens: Option<u32>,
}

impl TokenUsage {
    /// 累加另一份 TokenUsage 到自身（None 视为 0，保留"维度是否被报告"的语义）
    pub fn add(&mut self, other: &TokenUsage) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(other.prompt_tokens);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(other.completion_tokens);
        self.total_tokens = self.total_tokens.saturating_add(other.total_tokens);
        self.reasoning_tokens = match (self.reasoning_tokens, other.reasoning_tokens) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        self.cache_hit_tokens = match (self.cache_hit_tokens, other.cache_hit_tokens) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }
}

/// 持久化 JSONL 中的一条用量记录（供 UsageTrackerTrait 使用，定义放在这里方便跨包共享）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub session_id: String,
    pub request_id: String,
    pub trace_id: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_tokens: Option<u32>,
    pub timestamp: String,
}

/// 聚合视图（从多条 UsageRecord 汇总得到）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub reasoning_tokens: u32,
    #[serde(default)]
    pub cache_hit_tokens: u32,
    pub record_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_timestamp: Option<String>,
}

/// Session 维度用量视图（带上下文大小与窗口上限）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageView {
    pub session_id: String,
    pub snapshot: UsageSnapshot,
    /// 本 session 累计 prompt_tokens（用于上下文压缩检测）
    pub context_size_tokens: u32,
    /// Provider 配置的上下文窗口 token 上限（未知则为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx_window_tokens: Option<u32>,
}

/// Provider/Model 维度用量视图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsageView {
    pub provider: String,
    pub model: String,
    pub snapshot: UsageSnapshot,
}

/// 全局用量总览
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalUsageView {
    pub total: UsageSnapshot,
    #[serde(default)]
    pub by_provider_model: Vec<ProviderUsageView>,
    #[serde(default)]
    pub by_session: Vec<SessionUsageView>,
}

/// 消息角色枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }
}

impl std::str::FromStr for MessageRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "system" => Ok(MessageRole::System),
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "tool" => Ok(MessageRole::Tool),
            _ => Err(format!("unknown role: {}", s)),
        }
    }
}

/// 聊天消息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System.as_str().into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User.as_str().into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant.as_str().into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant.as_str().to_string(),
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool.as_str().to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// 聊天响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub id: String,
    pub tool_calls: Vec<ToolCall>,
}

impl ChatResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    pub fn get_content(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
}

/// 聊天响应块结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    pub reasoning_content: Option<String>,
    pub content: Option<String>,
    pub id: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// LLM提供者特质
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    fn config(&self) -> Arc<ProviderConfig>;

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>;

    /// 返回最近一次调用的 usage（若 provider 支持）；默认实现返回 None
    async fn last_usage(&self) -> Option<TokenUsage> {
        None
    }
}

/// LLM类型枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmType {
    OpenAI,
}

/// LLM提供者配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub llm_type: LlmType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub models: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub options: serde_json::Value,
    /// 上下文窗口 token 上限（如 128000），用于上下文压缩判断
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx_window_tokens: Option<u32>,
    /// 单次输出最大 token（如 4096）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

impl ProviderConfig {
    pub fn default_model(&self) -> &str {
        if let Some(model) = &self.default_model {
            return model.as_str();
        }

        self.models
            .iter()
            .min_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key))
            .map(|(_, model)| model.as_str())
            .unwrap_or("")
    }
}

/// LLM配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub model_name: String,
}
