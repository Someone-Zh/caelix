use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub current: u32,
    pub total: u32,
}

impl TaskProgress {
    pub fn percentage(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.current as f32 / self.total as f32
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: TaskStatus,
    pub progress: Option<TaskProgress>,
    pub created_at: DateTime<Utc>,
}
