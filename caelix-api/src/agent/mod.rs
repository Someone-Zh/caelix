//! Agent 核心定义模块
//!
//! 包含 AgentSpec 和 AgentOutputChunk 的定义

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

use crate::provider::ChatMessage;
use crate::tool::Tool;
use crate::{AgentError, LlmConfig, LlmProvider};

/// Agent 输出流分片
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentOutputChunk {
    Start {
        timestamp: DateTime<Utc>,
    },
    CallProvider {
        timestamp: DateTime<Utc>,
        provider: String,
        model: String,
    },
    Reasoning {
        content: String,
    },
    Content {
        content: String,
    },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        tool_name: String,
        result: String,
    },
    Finish {
        reason: String,
    },
}

impl std::fmt::Display for AgentOutputChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentOutputChunk::Start { .. } => write!(f, ""),
            AgentOutputChunk::CallProvider { .. } => write!(f, ""),
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
    pub system_prompt: Arc<String>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub group: Option<Arc<String>>,
}

impl AgentSpec {
    pub fn new(name: String, system_prompt: String, tools: Vec<Arc<dyn Tool>>) -> Self {
        Self {
            name,
            system_prompt: Arc::new(system_prompt),
            tools,
            group: None,
        }
    }

    /// 创建带group的AgentSpec
    pub fn with_group(
        name: String,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
        group: Option<String>,
    ) -> Self {
        Self {
            name,
            system_prompt: Arc::new(system_prompt),
            tools,
            group: group.map(Arc::new),
        }
    }

    /// 获取工具定义列表
    pub fn get_tool_definitions(&self) -> Vec<crate::tool::ToolDefinition> {
        self.tools.iter().map(|t| t.to_definition()).collect()
    }
}

impl AgentSpec {
    pub fn build_messages(&self, user_input: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage::system(self.system_prompt.as_str())];
        messages.extend(user_input);
        messages
    }
}

#[async_trait]
pub trait Agent: Send + Sync {
    async fn run(
        &self,
        messages: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: &LlmConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>>;
    fn get_spec(&self) -> Arc<AgentSpec>;
}
