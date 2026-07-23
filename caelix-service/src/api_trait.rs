//! API Trait 定义
#![allow(dead_code)]

use crate::types::{
    ChatAsyncResult, ChatRequest, HookInfo, PluginInfo, ProviderInfo, SessionSummary,
    SecurityCheckerInfo, SkillInfo, ToolInfo, AgentSpecInfo, CommandInfo,
    ToolExecuteResult, MemoryRecallResult, MemoryStats, MemoryAxiom,
    MemoryConflict, MemoryCandidate, MemoryBudgetInfo,
};
use async_trait::async_trait;
use caelix_api::error::ApiError;
use caelix_api::message::{AgentMessage, NotificationMessage};
use caelix_api::provider::{GlobalUsageView, SessionUsageView};
use caelix_api::task::TaskMeta;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;

#[async_trait]
pub trait CaelixApi: Send + Sync {
    // ==================== 会话管理 ====================

    /// 创建新会话，返回 session_id
    async fn create_session(&self) -> Result<String, ApiError>;

    /// 使用指定的 session_id 创建会话（如果不存在）
    async fn create_session_with_id(&self, session_id: String) -> Result<(), ApiError>;

    /// 检查会话是否存在
    async fn session_exists(&self, session_id: &str) -> Result<bool, ApiError>;

    /// 获取会话列表
    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError>;

    /// 获取会话的完整 Agent 消息历史
    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, ApiError>;

    /// 设置会话的提供者
    async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<(), ApiError>;

    /// 设置会话的模型
    async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), ApiError>;

    /// 获取指定 session 的累计 Token 用量
    async fn get_session_usage(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionUsageView>, ApiError>;

    /// 紧急停止指定 session 中正在运行的 Agent
    async fn stop_agent(&self, session_id: &str) -> Result<bool, ApiError>;

    // ==================== 变量管理 ====================

    /// 设置全局变量
    async fn set_variable(&self, key: &str, value: &str) -> Result<(), ApiError>;

    /// 获取全局变量
    async fn get_variable(&self, key: &str) -> Result<Option<String>, ApiError>;

    /// 删除全局变量
    async fn delete_variable(&self, key: &str) -> Result<(), ApiError>;

    /// 列出所有全局变量
    async fn list_variables(&self) -> Result<HashMap<String, String>, ApiError>;

    /// 设置空间变量
    async fn set_space_variable(&self, space: &str, key: &str, value: &str) -> Result<(), ApiError>;

    /// 获取空间变量
    async fn get_space_variable(&self, space: &str, key: &str) -> Result<Option<String>, ApiError>;

    /// 删除空间变量
    async fn delete_space_variable(&self, space: &str, key: &str) -> Result<(), ApiError>;

    /// 列出空间的所有变量
    async fn list_space_variables(&self, space: &str) -> Result<HashMap<String, String>, ApiError>;

    /// 替换文本中的变量
    async fn replace_variables(&self, text: &str, space: Option<&str>) -> Result<String, ApiError>;

    // ==================== 智能体配置 ====================

    /// 获取默认提供者（None 表示未配置）
    fn get_default_provider(&self) -> Option<String>;

    /// 获取默认模型（None 表示未配置）
    fn get_default_model(&self) -> Option<String>;

    /// 获取所有提供者及模型信息
    async fn get_providers(&self) -> Result<Vec<ProviderInfo>, ApiError>;

    /// 获取指定提供者的模型列表
    async fn get_provider_models(&self, provider_name: &str) -> Result<Vec<String>, ApiError>;

    /// 获取所有 agent 名称列表
    async fn list_agents(&self) -> Vec<String>;

    /// 获取所有 agent 详细信息
    async fn list_agents_info(&self) -> Result<Vec<AgentSpecInfo>, ApiError>;

    /// 获取指定 agent 的详细信息
    async fn get_agent_info(&self, name: &str) -> Result<Option<AgentSpecInfo>, ApiError>;

    // ==================== 聊天与对话 ====================

    /// 异步触发聊天流（后台执行）
    async fn chat_stream_async(&self, request: ChatRequest) -> Result<ChatAsyncResult, ApiError>;

    /// 订阅聊天流
    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError>;

    /// 审批指定 tool_call
    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), ApiError>;

    // ==================== 任务管理 ====================

    /// 获取任务列表
    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError>;

    // ==================== 用量管理 ====================

    /// 获取全局 Token 用量（按 provider/model 维度汇总）
    async fn get_global_usage(&self) -> Result<GlobalUsageView, ApiError>;

    // ==================== 通知管理 ====================

    /// 获取会话通知历史
    async fn get_session_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<NotificationMessage>, ApiError>;

    // ==================== 安全检查 ====================

    /// 获取安全检查器配置
    async fn get_security_config(&self) -> Result<SecurityCheckerInfo, ApiError>;

    /// 添加允许路径
    async fn add_path_include(&self, path: &str) -> Result<(), ApiError>;

    /// 添加排除路径
    async fn add_path_exclude(&self, path: &str) -> Result<(), ApiError>;

    /// 添加允许 URL 模式
    async fn add_url_include(&self, pattern: &str) -> Result<(), ApiError>;

    /// 添加排除 URL 模式
    async fn add_url_exclude(&self, pattern: &str) -> Result<(), ApiError>;

    /// 添加允许命令
    async fn add_command_include(&self, command: &str) -> Result<(), ApiError>;

    /// 添加排除命令
    async fn add_command_exclude(&self, command: &str) -> Result<(), ApiError>;

    /// 检查路径是否安全
    async fn check_path(&self, path: &str) -> Result<bool, ApiError>;

    /// 检查 URL 是否安全
    async fn check_url(&self, url: &str) -> Result<bool, ApiError>;

    /// 检查命令是否安全
    async fn check_command(&self, command: &str) -> Result<bool, ApiError>;

    // ==================== 技能管理 ====================

    /// 获取所有技能列表
    async fn list_skills(&self) -> Result<Vec<SkillInfo>, ApiError>;

    /// 获取所有技能名称列表
    async fn list_skill_names(&self) -> Result<Vec<String>, ApiError>;

    /// 获取指定技能信息
    async fn get_skill_info(&self, name: &str) -> Result<Option<SkillInfo>, ApiError>;

    /// 获取项目级技能列表
    async fn list_project_skills(&self, work_dir: &str) -> Result<Vec<SkillInfo>, ApiError>;

    // ==================== 命令管理 ====================

    /// 获取所有命令列表
    async fn list_commands(&self) -> Result<Vec<CommandInfo>, ApiError>;

    /// 获取指定命令信息
    async fn get_command_info(&self, name: &str) -> Result<Option<CommandInfo>, ApiError>;

    /// 获取项目级命令列表
    async fn list_project_commands(&self, work_dir: &str) -> Result<Vec<CommandInfo>, ApiError>;

    // ==================== 工具管理 ====================

    /// 获取所有工具列表
    async fn list_tools(&self) -> Result<Vec<ToolInfo>, ApiError>;

    /// 获取所有工具名称列表
    async fn list_tool_names(&self) -> Result<Vec<String>, ApiError>;

    /// 获取指定工具信息
    async fn get_tool_info(&self, name: &str) -> Result<Option<ToolInfo>, ApiError>;

    /// 执行指定工具
    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolExecuteResult, ApiError>;

    // ==================== 记忆包管理 ====================

    /// 记忆检索
    async fn memory_recall(
        &self,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<MemoryRecallResult>, ApiError>;

    /// 写入 Raw 层记忆
    async fn memory_write(
        &self,
        content: &str,
        source: &str,
        tags: Vec<String>,
    ) -> Result<(), ApiError>;

    /// 手动触发 Raw→Wiki 晋升
    async fn memory_promote_raw(&self, file: &str) -> Result<(), ApiError>;

    /// 手动触发 Wiki→Axiom 晋升
    async fn memory_promote_wiki(&self, entity: &str) -> Result<(), ApiError>;

    /// 列出冲突
    async fn memory_list_conflicts(&self, all: bool) -> Result<Vec<MemoryConflict>, ApiError>;

    /// 列出 Axiom 候选
    async fn memory_list_candidates(&self, all: bool) -> Result<Vec<MemoryCandidate>, ApiError>;

    /// 重建反向索引
    async fn memory_rebuild_index(&self) -> Result<(), ApiError>;

    /// 获取记忆统计
    async fn memory_stats(&self) -> Result<MemoryStats, ApiError>;

    /// 列出 Axiom
    async fn memory_list_axioms(
        &self,
        include_deprecated: bool,
    ) -> Result<Vec<MemoryAxiom>, ApiError>;

    /// 获取 LLM 预算信息
    async fn memory_budget(&self) -> Result<MemoryBudgetInfo, ApiError>;

    // ==================== Hook 管理 ====================

    /// 获取所有注册的 AgentHook 列表
    async fn list_hooks(&self) -> Result<Vec<HookInfo>, ApiError>;

    /// 获取指定 Hook 信息
    async fn get_hook_info(&self, name: &str) -> Result<Option<HookInfo>, ApiError>;

    // ==================== 插件管理 ====================

    /// 获取所有注册的插件列表
    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, ApiError>;

    /// 获取指定插件信息
    async fn get_plugin_info(&self, name: &str) -> Result<Option<PluginInfo>, ApiError>;
}
