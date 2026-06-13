//! API Trait 定义
#![allow(dead_code)]

use crate::types::{ChatAsyncResult, ChatRequest, ProviderInfo, SessionSummary};
use async_trait::async_trait;
use caelix_api::agent::AgentOutputChunk;
use caelix_api::error::ApiError;
use caelix_api::message::{AgentMessage, NotificationMessage};
use caelix_api::task::TaskMeta;
use futures::Stream;
use futures::stream::BoxStream;
use std::pin::Pin;

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
    async fn create_session(&self) -> String;

    /// 使用指定的session_id创建会话（如果不存在）
    async fn create_session_with_id(&self, session_id: String);

    /// 检查会话是否存在
    async fn session_exists(&self, session_id: &str) -> bool;

    /// 获取所有 agent 名称列表
    async fn list_agents(&self) -> Vec<String>;

    /// 流式聊天
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<AgentOutputChunk, ApiError>>, ApiError>;

    /// 获取会话的完整 Agent 消息历史
    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, ApiError>;

    /// 获取任务列表
    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError>;

    /// 获取会话列表
    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError>;

    /// 获取所有提供者及模型信息
    async fn get_providers(&self) -> Result<Vec<ProviderInfo>, ApiError>;

    /// 获取指定提供者的模型列表
    async fn get_provider_models(&self, provider_name: &str) -> Result<Vec<String>, ApiError>;

    /// 获取会话通知历史
    async fn get_session_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<NotificationMessage>, ApiError>;

    /// 异步触发聊天流（后台执行）
    ///
    /// 该方法会立即返回，聊天过程在后台异步执行
    /// 所有流式输出块会通过消息总线以 AgentMessageType::Chunk 类型发送
    /// 调用方可通过 subscribe_chat_stream 订阅结果
    async fn chat_stream_async(&self, request: ChatRequest) -> Result<ChatAsyncResult, ApiError>; // 返回 request_id、span_id 和 session_id

    /// 订阅聊天流
    ///
    /// 返回一个 Stream，持续接收指定 session 的 Agent 消息
    /// 可以通过取消 Stream 来主动断开订阅
    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError>;

    /// 审批指定 tool_call：
    /// - 从会话历史下方找到对应 Assistant 消息（含 tool_calls）中的指定项，
    ///   标记其审批状态；
    /// - 若 approved=true，则实际执行该工具并追加 tool_result；
    /// - 若 approved=false，则追加一条拒绝文本 tool_result。
    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), ApiError>;
}
