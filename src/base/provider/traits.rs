use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;
use serde::{Deserialize, Serialize};
use crate::base::AgentError;
use crate::base::tool::{ToolCall,ToolDefinition};
use std::sync::Arc;
use std::collections::HashMap;

/// LLM (Large Language Model) 相关的核心数据结构和接口定义
/// 对应架构：第一层 - 核心层
/// 该模块定义了与LLM交互所需的基本数据类型和抽象接口


/// 消息角色枚举
/// 定义了在对话中不同参与者的角色
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")] 
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}
impl MessageRole {
    // 枚举转字符串（核心方法）
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }

    // 可选：字符串转枚举（校验用，防止非法值）
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "system" => Some(MessageRole::System),
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "tool" => Some(MessageRole::Tool),
            _ => None,
        }
    }
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System.as_str().into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User.as_str().into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant.as_str().into(),
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool.as_str().into(),
            content: content.into(),
        }
    }
}



/// 聊天响应结构体
/// 表示LLM的完整响应，包含生成的内容或工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// 生成的文本内容，可能为None（当响应仅包含工具调用时）
    pub content: Option<String>,
    /// 思考过程，可能为None
    pub reasoning_content: Option<String>,
    /// 响应的唯一标识符
    pub id: String,
    /// 工具调用列表，可能为None（当响应仅包含文本内容时）
    pub tool_calls: Vec<ToolCall>,
}
impl ChatResponse {
    /// 判断是否有工具调用列表
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// 获取生成的文本内容，若为空则返回空字符串
    pub fn get_content(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
}

/// 聊天响应块结构体
/// 用于流式输出时的部分响应，包含增量内容或工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    /// 思考信息
    pub reasoning_content: Option<String>,
    /// 增量生成的文本内容，可能为None
    pub content: Option<String>,
    /// 响应的唯一标识符（与完整响应相同）
    pub id: String,
    /// 工具调用列表，可能为None
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 完成原因，仅在最后一个块中提供
    pub finish_reason: Option<String>,
}

/// LLM提供者特质
/// 定义了与不同LLM服务交互的统一接口
/// 对应架构：第一层 - 核心层
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {

    fn config(&self) -> Arc<ProviderConfig>;

    /// 流式对话接口
    /// 用于实时获取LLM的生成结果，提供更好的用户体验
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError>;
    
}


/// LLM类型枚举
/// 定义了支持的LLM服务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmType {
    /// OpenAI服务
    OpenAI,
}

/// LLM提供者配置结构体
/// 定义了LLM提供者的配置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供者名称，用于在管理器中标识
    pub name: String,
    /// LLM服务类型
    pub llm_type: LlmType,
    /// API密钥，用于验证身份
    pub api_key: String,
    /// 基础URL，用于自定义API端点
    /// 为None时使用默认URL
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// 模型映射，将通用模型名称映射到具体服务的模型名称
    pub models: HashMap<String, String>,
    /// 额外选项，以JSON格式存储
    pub options: serde_json::Value,
}

impl ProviderConfig {

    pub fn default_model(&self) -> &str {
        self.models
            .values()
            .next()
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

/// LLM配置结构体
/// 定义了与LLM交互时的配置参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 使用的模型名称
    pub model_name: String,
}