//! API 类型定义
//! 这些类型是公共API的一部分，可能被外部使用
#![allow(dead_code)]

use caelix_api::message::{AgentMessage, NotificationMessage};
use caelix_api::task::TaskMeta;
use caelix_security::config::SecurityConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 聊天请求
#[derive(Debug, Deserialize, Clone)]
pub struct ChatRequest {
    pub session_id: String,
    /// 若为 None，则视为 "继续" 或 "恢复流程" 触发：
    /// 此时若会话最后一条消息是 Assistant 且含 tool_calls，则进入 resume 路径。
    pub message: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    /// 用户项目根目录，用于加载项目级配置（skills/commands/agents）和变量替换的 space。
    /// 若为 None，则回退到服务进程的 current_dir()。
    pub work_dir: Option<String>,
}

/// 会话创建响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

/// 默认配置响应
#[derive(Debug, Serialize)]
pub struct DefaultConfigResponse {
    pub default_provider: String,
    pub default_model: String,
}

/// Agent 列表响应
#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<String>,
}

/// 会话消息列表响应
#[derive(Debug, Serialize)]
pub struct SessionMessagesResponse {
    pub messages: Vec<AgentMessage>,
}

/// 会话通知列表响应
#[derive(Debug, Serialize)]
pub struct SessionNotificationsResponse {
    pub notifications: Vec<NotificationMessage>,
}

/// 任务列表响应
#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskMeta>,
}

/// 任务查询参数
#[derive(Debug, Deserialize)]
pub struct TaskQueryParams {
    pub session_id: Option<String>,
}

/// 会话摘要信息
#[derive(Debug, Serialize, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub summary: String, // 首次输入的前15个字符
}

/// 提供者信息
#[derive(Debug, Serialize, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub llm_type: String,
    pub models: Vec<String>,
}

/// 异步聊天响应
#[derive(Debug, Serialize, Clone)]
pub struct ChatAsyncResult {
    pub request_id: String,
    pub span_id: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SecurityCheckerInfo {
    pub config: SecurityConfig,
}

#[derive(Debug, Serialize, Clone)]
pub struct HookInfo {
    pub name: String,
    pub capabilities: String,
    pub scope: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub capabilities: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub namespace: String,
    pub full_name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub triggers: Vec<SkillTriggerInfo>,
    pub globs: Vec<String>,
    pub disable_model_invocation: bool,
    pub user_invocable: bool,
    pub argument_hint: Option<String>,
    pub compatibility: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SkillTriggerInfo {
    pub trigger_type: String,
    pub name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct AgentSpecInfo {
    pub name: String,
    pub group: Option<String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub command_type: String,
    pub description: String,
}

/// 工具执行请求
#[derive(Debug, Deserialize, Clone)]
pub struct ToolExecuteRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 工具执行结果
#[derive(Debug, Serialize, Clone)]
pub struct ToolExecuteResult {
    pub output: String,
    pub error: Option<String>,
}

/// 记忆检索结果
#[derive(Debug, Serialize, Clone)]
pub struct MemoryRecallResult {
    pub layer: String,
    pub file: String,
    pub heading: String,
    pub preview: String,
    pub confidence: Option<f64>,
}

/// 记忆统计信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryStats {
    pub raw_files: usize,
    pub wiki_entities: usize,
    pub wiki_events: usize,
    pub axioms: usize,
    pub axioms_active: usize,
    pub pending_conflicts: usize,
    pub pending_candidates: usize,
    pub pending_links: usize,
    pub llm_budget_used: u32,
    pub llm_budget_total: u32,
}

/// Axiom 信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryAxiom {
    pub name: String,
    pub category: String,
    pub status: String,
    pub confidence: f64,
    pub created_at: String,
    pub deprecated_reason: Option<String>,
}

/// 冲突信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryConflict {
    pub id: String,
    pub r#type: String,
    pub entity: String,
    pub field: Option<String>,
    pub status: String,
    pub values: Vec<String>,
}

/// 候选 Axiom 信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryCandidate {
    pub id: String,
    pub confidence: f64,
    pub status: String,
    pub preview: String,
}

/// LLM 预算信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryBudgetInfo {
    pub used: u32,
    pub budget: u32,
    pub remaining: u32,
    pub exhausted: bool,
}
