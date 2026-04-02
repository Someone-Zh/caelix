use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;
use serde::{Deserialize, Serialize};
use crate::base::AgentError;
/// LLM (Large Language Model) 相关的核心数据结构和接口定义
/// 对应架构：第一层 - 核心层
/// 该模块定义了与LLM交互所需的基本数据类型和抽象接口

/// 消息角色枚举
/// 定义了在对话中不同参与者的角色
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    /// 用户角色，代表用户输入的消息
    User,
    /// 助手角色，代表AI助手的回复
    Assistant,
    /// 系统角色，代表系统级别的指令或上下文信息
    System,
}

/// 消息结构体
/// 表示对话中的一条消息，包含角色和内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息发送者的角色
    pub role: MessageRole,
    /// 消息的具体内容
    pub content: String,
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

/// 聊天响应结构体
/// 表示LLM的完整响应，包含生成的内容或工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// 生成的文本内容，可能为None（当响应仅包含工具调用时）
    pub content: Option<String>,
    /// 响应的唯一标识符
    pub id: String,
    /// 工具调用列表，可能为None（当响应仅包含文本内容时）
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// 聊天响应块结构体
/// 用于流式输出时的部分响应，包含增量内容或工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseChunk {
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
    /// 流式对话接口
    /// 用于实时获取LLM的生成结果，提供更好的用户体验
    /// 
    /// # 参数
    /// - `messages`: 对话历史消息列表
    /// - `config`: LLM配置参数
    /// 
    /// # 返回值
    /// - `Result`: 包含流式响应的结果，每个流项是一个`ChatResponseChunk`
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError>;
    
    /// 非流式对话接口
    /// 用于一次性获取LLM的完整响应
    /// 
    /// # 参数
    /// - `messages`: 对话历史消息列表
    /// - `config`: LLM配置参数
    /// 
    /// # 返回值
    /// - `Result`: 包含完整`ChatResponse`的结果
    async fn chat(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<ChatResponse, AgentError>;
}

/// LLM配置结构体
/// 定义了与LLM交互时的配置参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 生成文本的温度参数，控制输出的随机性
    /// 值越高，输出越随机；值越低，输出越确定
    pub temperature: f32,
    /// 最大生成令牌数，限制输出长度
    /// 为None时使用模型默认值
    pub max_tokens: Option<u32>,
    /// 使用的模型名称
    pub model_name: String,
}