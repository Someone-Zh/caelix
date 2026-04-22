use async_trait::async_trait;
use futures::stream::BoxStream;
use crate::api::types::{ApiError, ChatRequest};
use crate::base::agent::AgentOutputChunk;

/// Caelix API trait
/// 定义了对外提供的统一接口
#[async_trait]
pub trait CaelixApi: Send + Sync {
    /// 获取默认提供者
    fn get_default_provider(&self) -> String;
    
    /// 获取默认模型
    fn get_default_model(&self) -> String;
    
    /// 设置会话的提供者
    async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<(), ApiError>;
    
    /// 设置会话的模型
    async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), ApiError>;
    
    /// 创建新会话
    fn create_session(&self) -> String;
    
    /// 获取所有 agent 名称列表
    async fn list_agents(&self) -> Vec<String>;
    
    /// 流式聊天
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<AgentOutputChunk, ApiError>>, ApiError>;
}