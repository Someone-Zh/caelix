//! 上下文提供者接口抽象
//! 
//! 定义轻量级的接口 trait，允许运行时层通过统一接口访问配置层的组件，
//! 避免 caelix-runtime 直接依赖 caelix-config

use std::sync::Arc;
use async_trait::async_trait;
use anyhow::Result;

use crate::hooks::{MessageUpdateContext, PreToolExecContext, PostToolExecContext};
use crate::message::{AgentMessage, TaskMessage};

/// Hook 执行器接口
/// 
/// 抽象 HookRegistry 的核心功能，避免在 api 层暴露具体类型
#[async_trait]
pub trait HookExecutor: Send + Sync {
    /// 执行消息更新钩子
    async fn execute_message_update(
        &self,
        ctx: &MessageUpdateContext,
    ) -> Result<()>;
    
    /// 执行工具执行前钩子
    async fn execute_pre_tool_exec(
        &self,
        ctx: &mut PreToolExecContext,
    ) -> Result<()>;
    
    /// 执行工具执行后钩子
    async fn execute_post_tool_exec(
        &self,
        ctx: &mut PostToolExecContext,
    ) -> Result<()>;
}

/// 消息发送器接口
/// 
/// 抽象 MessageBus 的核心功能
pub trait MessageSender: Send + Sync {
    /// 发送 Agent 消息
    fn send_agent(&self, message: AgentMessage) -> Result<()>;
    
    /// 发送任务消息
    fn send_task(&self, message: TaskMessage) -> Result<()>;
}

/// 上下文提供者 Trait
/// 
/// 允许运行时层通过统一接口访问配置层的组件
/// 避免 caelix-runtime 直接依赖 caelix-config
pub trait ContextProvider: Send + Sync {
    /// 获取 Hook 执行器
    fn get_hook_executor(&self) -> Arc<dyn HookExecutor>;
    
    /// 获取消息发送器
    fn get_message_sender(&self) -> Arc<dyn MessageSender>;
    
    /// 获取默认 Provider 名称
    fn get_default_provider(&self) -> &str;
    
    /// 获取默认 Model 名称
    fn get_default_model(&self) -> &str;
}

/// RuntimeContext trait - 提供运行时上下文信息
/// 
/// 注意：这是一个trait定义，具体实现在 caelix-runtime 包中
pub trait RuntimeContextTrait: Send + Sync {
    /// 获取 CaelixContext（需要转换为具体的类型）
    fn get_context(&self) -> Arc<dyn std::any::Any>;
    
    /// 获取当前会话ID
    fn session_id(&self) -> String;
    
    /// 获取当前请求ID
    fn request_id(&self) -> String;
    
    /// 获取当前span ID
    fn span_id(&self) -> String;
}
