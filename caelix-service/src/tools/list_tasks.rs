use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use caelix_api::tool::{ToolResult, Tool};
use caelix_task::TaskManager;

/// 任务列表工具
pub struct ListTasksTool {
    task_manager: Arc<TaskManager>,
}

impl std::fmt::Debug for ListTasksTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListTasksTool").finish()
    }
}

impl Clone for ListTasksTool {
    fn clone(&self) -> Self {
        Self {
            task_manager: self.task_manager.clone(),
        }
    }
}

impl ListTasksTool {
    pub fn new(task_manager: Arc<TaskManager>) -> Self {
        Self { task_manager }
    }
}

#[async_trait]
impl Tool for ListTasksTool {
    fn name(&self) -> &str {
        "list_tasks"
    }

    fn description(&self) -> &str {
        "获取任务列表。支持按会话过滤。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": ["string", "null"],
                    "description": "可选，指定要查询的会话ID"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let session_id = input["session_id"].as_str().map(|s| s.to_string());

        // 列出所有任务（支持按会话过滤）
        let tasks = self.task_manager.list_tasks(session_id.as_deref()).await;

        if tasks.is_empty() {
            ToolResult {
                output: "No tasks found".to_string(),
                error: None,
            }
        } else {
            let task_list: Vec<String> = tasks
                .iter()
                .map(|t| {
                    format!(
                        "Task {}: type={}, status={:?}, session={}",
                        t.task_id, t.task_type_name, t.status, t.session_id
                    )
                })
                .collect();

            ToolResult {
                output: task_list.join("\n"),
                error: None,
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
