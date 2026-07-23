//! API 核心实现模块
//!
//! 按功能分类拆分为多个子模块，每个子模块实现 `CaelixApi` trait 的对应分组方法。
//! 本模块仅保留结构体定义、公共辅助函数，以及单一 `impl CaelixApi` 委托块。

mod agent;
mod chat;
mod command;
mod hook;
mod memory;
mod notification;
mod plugin;
mod security;
mod session;
mod skill;
mod task;
mod tool;
mod usage;
mod variable;

use async_trait::async_trait;
use caelix_api::error::ApiError;
use caelix_api::message::{AgentMessage, AgentMessageType};
use caelix_api::provider::ChatMessage;
use caelix_runtime::context::CaelixContext;
use std::sync::Arc;

use crate::api_trait::CaelixApi;
use crate::types::{
    AgentSpecInfo, ChatAsyncResult, ChatRequest, CommandInfo, HookInfo, PluginInfo, ProviderInfo,
    SecurityCheckerInfo, SessionSummary, SkillInfo, ToolExecuteResult, ToolInfo,
    MemoryRecallResult, MemoryStats, MemoryAxiom, MemoryConflict, MemoryCandidate,
    MemoryBudgetInfo,
};
use caelix_api::message::NotificationMessage;
use caelix_api::provider::GlobalUsageView;
use caelix_api::provider::SessionUsageView;
use caelix_api::task::TaskMeta;

/// 会话摘要截取的最大字符数
pub const SUMMARY_MAX_CHARS: usize = 15;

/// 工具执行超时时间（秒）
pub const TOOL_EXECUTION_TIMEOUT_SECS: u64 = 300;

/// list_sessions 最大并发数
pub const LIST_SESSIONS_MAX_CONCURRENCY: usize = 10;

/// 校验 session_id：非空且仅允许 [A-Za-z0-9_-]
///
/// 与 FileStorage / FileTaskStorage 的校验规则一致，防止路径穿越。
pub(crate) fn validate_session_id(session_id: &str) -> Result<(), ApiError> {
    if !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(ApiError::invalid_request(
            "session_id 非法：仅允许非空的 [A-Za-z0-9_-]",
        ))
    }
}

/// 构造 AgentMessage（审批场景辅助函数，request_id/span_id/trace_id 留空）
pub(crate) fn make_agent_message(
    session_id: &str,
    msg_type: AgentMessageType,
    content: String,
    agent_name: Option<String>,
) -> AgentMessage {
    AgentMessage {
        session_id: session_id.to_string(),
        request_id: String::new(),
        span_id: String::new(),
        trace_id: String::new(),
        r#type: msg_type,
        timestamp: chrono::Utc::now(),
        content,
        agent_name,
        usage: None,
    }
}

/// 从消息列表中提取首条消息作为摘要
///
/// 注意：调用方应尽可能只传入必要的消息（如前 N 条），避免全量加载。
pub(crate) fn summarize_first_message(messages: &[AgentMessage]) -> String {
    messages
        .first()
        .map(|msg| {
            let actual_content =
                serde_json::from_str::<ChatMessage>(&msg.content)
                    .map(|cm| cm.content)
                    .unwrap_or_else(|_| msg.content.clone());

            let mut result = String::with_capacity(SUMMARY_MAX_CHARS + 3);
            for (i, ch) in actual_content.chars().enumerate() {
                if i >= SUMMARY_MAX_CHARS {
                    result.push_str("...");
                    break;
                }
                result.push(ch);
            }
            result
        })
        .unwrap_or_else(|| "新会话".to_string())
}

/// 执行 Agent
pub(crate) async fn run_agent(
    agent_spec: Arc<caelix_api::agent::AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &caelix_api::provider::LlmConfig,
) -> Result<String, anyhow::Error> {
    caelix_agent::run_agent(agent_spec, messages, provider, config)
        .await
        .map_err(|e| anyhow::anyhow!("Agent execution error: {:?}", e))
}

/// 构造并持久化一条 tool 结果消息，同时发送 Event 通知。
///
/// 这是 `execute_approved_tool` 和 `append_rejection_result` 的公共逻辑抽离。
pub(crate) async fn persist_and_notify_tool_result(
    context: &CaelixContext,
    session_id: &str,
    tool_call_id: &str,
    result_text: String,
    agent_name: Option<String>,
    event_text: String,
) -> Result<(), ApiError> {
    let chat_tool_msg = ChatMessage {
        role: "tool".to_string(),
        content: result_text,
        tool_calls: None,
        tool_call_id: Some(tool_call_id.to_string()),
    };

    let storage = context.session_manager.get_storage();
    let agent_msg = make_agent_message(
        session_id,
        AgentMessageType::Msg,
        serde_json::to_string(&chat_tool_msg).map_err(|e| {
            ApiError::InternalError(format!("序列化 tool result 失败: {}", e))
        })?,
        agent_name.clone(),
    );
    storage
        .append_agent_message(&agent_msg)
        .await
        .map_err(|e| ApiError::InternalError(format!("持久化 tool_result 失败: {}", e)))?;

    let event_msg = make_agent_message(
        session_id,
        AgentMessageType::Event,
        event_text,
        agent_name,
    );
    if context.message_bus.send_agent(event_msg).is_err() {
        tracing::warn!("Failed to send tool result event message to message bus");
    }

    Ok(())
}

/// API 核心实现
pub struct CaelixApiImpl {
    pub(crate) context: Arc<CaelixContext>,
    pub(crate) memory_service: memory::MemoryService,
}

impl CaelixApiImpl {
    pub fn new(context: Arc<CaelixContext>) -> Self {
        Self {
            context,
            memory_service: memory::MemoryService::new(),
        }
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<caelix_message::MessageBus> {
        &self.context.message_bus
    }

    /// 获取 SessionManager 引用
    pub fn session_manager(&self) -> &caelix_message::SessionManager {
        &self.context.session_manager
    }
}

/// 单一 `impl CaelixApi` 委托块：所有 trait 方法委托到对应子模块的自由函数。
///
/// Rust 不允许同一 type + 同一 trait 跨文件有多个 impl 块，
/// 因此采用「子模块提供自由函数 + 主模块单一 impl 委托」的拆分模式。
#[async_trait]
impl CaelixApi for CaelixApiImpl {
    // ==================== 会话管理 ====================

    async fn create_session(&self) -> Result<String, ApiError> {
        session::create_session(&self.context).await
    }

    async fn create_session_with_id(&self, session_id: String) -> Result<(), ApiError> {
        session::create_session_with_id(&self.context, session_id).await
    }

    async fn session_exists(&self, session_id: &str) -> Result<bool, ApiError> {
        session::session_exists(&self.context, session_id).await
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError> {
        session::list_sessions(&self.context).await
    }

    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, ApiError> {
        session::get_session_messages(&self.context, session_id).await
    }

    async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<(), ApiError> {
        session::set_session_provider(&self.context, session_id, provider).await
    }

    async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), ApiError> {
        session::set_session_model(&self.context, session_id, model).await
    }

    async fn get_session_usage(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionUsageView>, ApiError> {
        session::get_session_usage(&self.context, session_id).await
    }

    async fn stop_agent(&self, session_id: &str) -> Result<bool, ApiError> {
        session::stop_agent(&self.context, session_id).await
    }

    // ==================== 变量管理 ====================

    async fn set_variable(&self, key: &str, value: &str) -> Result<(), ApiError> {
        variable::set_variable(&self.context, key, value).await
    }

    async fn get_variable(&self, key: &str) -> Result<Option<String>, ApiError> {
        variable::get_variable(&self.context, key).await
    }

    async fn delete_variable(&self, key: &str) -> Result<(), ApiError> {
        variable::delete_variable(&self.context, key).await
    }

    async fn list_variables(&self) -> Result<std::collections::HashMap<String, String>, ApiError> {
        variable::list_variables(&self.context).await
    }

    async fn set_space_variable(
        &self,
        space: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ApiError> {
        variable::set_space_variable(&self.context, space, key, value).await
    }

    async fn get_space_variable(
        &self,
        space: &str,
        key: &str,
    ) -> Result<Option<String>, ApiError> {
        variable::get_space_variable(&self.context, space, key).await
    }

    async fn delete_space_variable(&self, space: &str, key: &str) -> Result<(), ApiError> {
        variable::delete_space_variable(&self.context, space, key).await
    }

    async fn list_space_variables(
        &self,
        space: &str,
    ) -> Result<std::collections::HashMap<String, String>, ApiError> {
        variable::list_space_variables(&self.context, space).await
    }

    async fn replace_variables(&self, text: &str, space: Option<&str>) -> Result<String, ApiError> {
        variable::replace_variables(&self.context, text, space).await
    }

    // ==================== 智能体配置 ====================

    fn get_default_provider(&self) -> Option<String> {
        agent::get_default_provider(&self.context)
    }

    fn get_default_model(&self) -> Option<String> {
        agent::get_default_model(&self.context)
    }

    async fn get_providers(&self) -> Result<Vec<ProviderInfo>, ApiError> {
        agent::get_providers(&self.context).await
    }

    async fn get_provider_models(&self, provider_name: &str) -> Result<Vec<String>, ApiError> {
        agent::get_provider_models(&self.context, provider_name).await
    }

    async fn list_agents(&self) -> Vec<String> {
        agent::list_agents(&self.context).await
    }

    async fn list_agents_info(&self) -> Result<Vec<AgentSpecInfo>, ApiError> {
        agent::list_agents_info(&self.context).await
    }

    async fn get_agent_info(&self, name: &str) -> Result<Option<AgentSpecInfo>, ApiError> {
        agent::get_agent_info(&self.context, name).await
    }

    // ==================== 聊天与对话 ====================

    async fn chat_stream_async(&self, request: ChatRequest) -> Result<ChatAsyncResult, ApiError> {
        chat::chat_stream_async(&self.context, request).await
    }

    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<std::pin::Pin<Box<dyn futures::Stream<Item = AgentMessage> + Send>>, ApiError> {
        chat::subscribe_chat_stream(&self.context, session_id).await
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), ApiError> {
        chat::approve_tool_call(&self.context, session_id, tool_call_id, approved).await
    }

    // ==================== 任务管理 ====================

    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError> {
        task::list_tasks(&self.context, session_id).await
    }

    // ==================== 用量管理 ====================

    async fn get_global_usage(&self) -> Result<GlobalUsageView, ApiError> {
        usage::get_global_usage(&self.context).await
    }

    // ==================== 通知管理 ====================

    async fn get_session_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<NotificationMessage>, ApiError> {
        notification::get_session_notifications(&self.context, session_id).await
    }

    // ==================== 安全检查 ====================

    async fn get_security_config(&self) -> Result<SecurityCheckerInfo, ApiError> {
        security::get_security_config(&self.context).await
    }

    async fn add_path_include(&self, path: &str) -> Result<(), ApiError> {
        security::add_path_include(&self.context, path).await
    }

    async fn add_path_exclude(&self, path: &str) -> Result<(), ApiError> {
        security::add_path_exclude(&self.context, path).await
    }

    async fn add_url_include(&self, pattern: &str) -> Result<(), ApiError> {
        security::add_url_include(&self.context, pattern).await
    }

    async fn add_url_exclude(&self, pattern: &str) -> Result<(), ApiError> {
        security::add_url_exclude(&self.context, pattern).await
    }

    async fn add_command_include(&self, command: &str) -> Result<(), ApiError> {
        security::add_command_include(&self.context, command).await
    }

    async fn add_command_exclude(&self, command: &str) -> Result<(), ApiError> {
        security::add_command_exclude(&self.context, command).await
    }

    async fn check_path(&self, path: &str) -> Result<bool, ApiError> {
        security::check_path(&self.context, path).await
    }

    async fn check_url(&self, url: &str) -> Result<bool, ApiError> {
        security::check_url(&self.context, url).await
    }

    async fn check_command(&self, command: &str) -> Result<bool, ApiError> {
        security::check_command(&self.context, command).await
    }

    // ==================== 技能管理 ====================

    async fn list_skills(&self) -> Result<Vec<SkillInfo>, ApiError> {
        skill::list_skills(&self.context).await
    }

    async fn list_skill_names(&self) -> Result<Vec<String>, ApiError> {
        skill::list_skill_names(&self.context).await
    }

    async fn get_skill_info(&self, name: &str) -> Result<Option<SkillInfo>, ApiError> {
        skill::get_skill_info(&self.context, name).await
    }

    async fn list_project_skills(&self, work_dir: &str) -> Result<Vec<SkillInfo>, ApiError> {
        skill::list_project_skills(&self.context, work_dir).await
    }

    // ==================== 命令管理 ====================

    async fn list_commands(&self) -> Result<Vec<CommandInfo>, ApiError> {
        command::list_commands(&self.context).await
    }

    async fn get_command_info(&self, name: &str) -> Result<Option<CommandInfo>, ApiError> {
        command::get_command_info(&self.context, name).await
    }

    async fn list_project_commands(&self, work_dir: &str) -> Result<Vec<CommandInfo>, ApiError> {
        command::list_project_commands(&self.context, work_dir).await
    }

    // ==================== 工具管理 ====================

    async fn list_tools(&self) -> Result<Vec<ToolInfo>, ApiError> {
        tool::list_tools(&self.context).await
    }

    async fn list_tool_names(&self) -> Result<Vec<String>, ApiError> {
        tool::list_tool_names(&self.context).await
    }

    async fn get_tool_info(&self, name: &str) -> Result<Option<ToolInfo>, ApiError> {
        tool::get_tool_info(&self.context, name).await
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolExecuteResult, ApiError> {
        tool::execute_tool(&self.context, tool_name, arguments).await
    }

    // ==================== 记忆包管理 ====================

    async fn memory_recall(
        &self,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<MemoryRecallResult>, ApiError> {
        memory::memory_recall(&self.memory_service, query, top_k).await
    }

    async fn memory_write(
        &self,
        content: &str,
        source: &str,
        tags: Vec<String>,
    ) -> Result<(), ApiError> {
        memory::memory_write(&self.memory_service, content, source, tags).await
    }

    async fn memory_promote_raw(&self, file: &str) -> Result<(), ApiError> {
        memory::memory_promote_raw(&self.memory_service, file).await
    }

    async fn memory_promote_wiki(&self, entity: &str) -> Result<(), ApiError> {
        memory::memory_promote_wiki(&self.memory_service, entity).await
    }

    async fn memory_list_conflicts(&self, all: bool) -> Result<Vec<MemoryConflict>, ApiError> {
        memory::memory_list_conflicts(&self.memory_service, all).await
    }

    async fn memory_list_candidates(&self, all: bool) -> Result<Vec<MemoryCandidate>, ApiError> {
        memory::memory_list_candidates(&self.memory_service, all).await
    }

    async fn memory_rebuild_index(&self) -> Result<(), ApiError> {
        memory::memory_rebuild_index(&self.memory_service).await
    }

    async fn memory_stats(&self) -> Result<MemoryStats, ApiError> {
        memory::memory_stats(&self.memory_service).await
    }

    async fn memory_list_axioms(
        &self,
        include_deprecated: bool,
    ) -> Result<Vec<MemoryAxiom>, ApiError> {
        memory::memory_list_axioms(&self.memory_service, include_deprecated).await
    }

    async fn memory_budget(&self) -> Result<MemoryBudgetInfo, ApiError> {
        memory::memory_budget(&self.memory_service).await
    }

    // ==================== Hook 管理 ====================

    async fn list_hooks(&self) -> Result<Vec<HookInfo>, ApiError> {
        hook::list_hooks(&self.context).await
    }

    async fn get_hook_info(&self, name: &str) -> Result<Option<HookInfo>, ApiError> {
        hook::get_hook_info(&self.context, name).await
    }

    // ==================== 插件管理 ====================

    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, ApiError> {
        plugin::list_plugins(&self.context).await
    }

    async fn get_plugin_info(&self, name: &str) -> Result<Option<PluginInfo>, ApiError> {
        plugin::get_plugin_info(&self.context, name).await
    }
}
