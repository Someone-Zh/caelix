use serde::{Serialize, Deserialize};
use uuid::Uuid;
use crate::core::llm::Message;

/// 任务种类枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// Agent 任务
    Agent,
}

/// 任务上下文 trait
#[async_trait::async_trait]
pub trait TaskContext: Send + Sync {
    /// 获取任务类型
    fn task_type(&self) -> TaskType;
}

/// Agent 任务上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskContext {
    /// Agent 名称
    pub agent_name: String,
    /// 消息列表
    pub messages: Vec<Message>,
}

#[async_trait::async_trait]
impl TaskContext for AgentTaskContext {
    fn task_type(&self) -> TaskType {
        TaskType::Agent
    }
}

/// 任务结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务 ID
    pub id: String,
    /// 任务名称
    pub name: String,
    /// 任务类型
    pub task_type: TaskType,
    /// 依赖的任务 ID
    pub parent_id: Option<String>,
    /// 任务上下文
    pub context: Box<dyn TaskContext>,
    /// 任务状态
    pub status: TaskStatus,
    /// 会话 ID
    pub session_id: String,
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// 待执行
    Pending,
    /// 执行中
    Running,
    /// 执行成功
    Completed,
    /// 执行失败
    Failed,
}

impl Task {
    /// 创建新任务
    pub fn new(name: String, task_type: TaskType, parent_id: Option<String>, context: Box<dyn TaskContext>, session_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            task_type,
            parent_id,
            context,
            status: TaskStatus::Pending,
            session_id,
        }
    }
}