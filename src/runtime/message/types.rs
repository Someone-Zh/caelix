use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    User,
    Agent,
    SubAgent,
    Tool,
    System,
}

/// 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    Thought,      // 思考过程
    ToolCall,     // 工具调用请求
    ToolResult,   // 工具执行结果
    Chunk,        // 流式内容块
    Status,       // 状态更新
    // 通用通知类型
    Info,         // 普通信息
    Error,        // 错误信息
    Warning,      // 警告信息
    Success,      // 成功信息
    // 任务相关类型
    TaskStarted,      // 任务开始
    TaskCompleted,    // 任务完成
    TaskFailed,       // 任务失败
    TaskProgress,     // 任务进度
}

/// 执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Done,
    Error,
}

/// 消息错误信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

/// 消息元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,  // 关联的任务ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,  // 流式消息组ID,同一组流式chunk共享此ID
    #[serde(default)]
    pub is_final: bool,              // 是否为流的最后一条消息
}

/// 核心消息结构 (OTEL 风格)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub session_id: String,       // TraceId: 整个会话ID
    pub span_id: String,          // SpanId: 当前步骤ID
    pub parent_span_id: Option<String>, // 父步骤ID
    pub seq: u64,                 // 全局序列号 (用于严格排序)
    
    pub role: Role,
    pub name: String,             // 执行者名称 (e.g., "SearchTool", "CodeAgent")
    pub r#type: MessageType,
    pub content: String,
    
    pub status: Status,
    pub timestamp: DateTime<Utc>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<MessageError>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<MessageMeta>,
}

impl Message {
    /// 便捷创建新消息 (需由 Bus 分配 seq)
    pub fn new(
        session_id: String,
        span_id: String,
        parent_span_id: Option<String>,
        role: Role,
        name: String,
        r#type: MessageType,
        content: String,
        status: Status,
    ) -> Self {
        Self {
            session_id,
            span_id,
            parent_span_id,
            seq: 0, // 占位，由 Bus 填充
            role,
            name,
            r#type,
            content,
            status,
            timestamp: Utc::now(),
            error: None,
            meta: None,
        }
    }
    
    /// 从当前 RuntimeContext 自动创建消息
    /// 
    /// 自动从运行时上下文获取 session_id 和 span_id
    /// 
    /// # Panics
    /// 如果在不存在的上下文中调用，会 panic
    /// 
    /// # Example
    /// ```no_run
    /// use caelix::runtime::message::{Message, Role, MessageType, Status};
    /// 
    /// let msg = Message::from_context(
    ///     None,
    ///     Role::User,
    ///     "user".to_string(),
    ///     MessageType::Chunk,
    ///     "Hello".to_string(),
    ///     Status::Running,
    /// );
    /// ```
    pub fn from_context(
        parent_span_id: Option<String>,
        role: Role,
        name: String,
        r#type: MessageType,
        content: String,
        status: Status,
    ) -> Self {
        use crate::runtime::context::RuntimeContext as Ctx;
        
        let session_id = Ctx::session_id();
        let span_id = Ctx::span_id();
        
        Self {
            session_id,
            span_id,
            parent_span_id,
            seq: 0, // 占位，由 Bus 填充
            role,
            name,
            r#type,
            content,
            status,
            timestamp: Utc::now(),
            error: None,
            meta: None,
        }
    }
    
    pub fn generate_span_id() -> String {
        Uuid::new_v4().to_string()
    }
}

/// Session 内存状态快照 (用于持久化 active_tasks)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub active_spans: std::collections::HashMap<String, ActiveSpanInfo>,
    /// 会话配置信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<SessionConfig>,
}

/// 会话配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl SessionConfig {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            provider: None,
            model: None,
            agent: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSpanInfo {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub status: Status,
    pub started_at: DateTime<Utc>,
}