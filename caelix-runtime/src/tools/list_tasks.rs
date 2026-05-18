use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;

use caelix_api::tool::{ToolResult, Tool};
use crate::RuntimeContext;
use caelix_message::task_message::TaskMessageType;
use caelix_task::types::TaskStatus;

/// 任务列表工具
pub struct ListTasksTool;

impl std::fmt::Debug for ListTasksTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListTasksTool").finish()
    }
}

impl Clone for ListTasksTool {
    fn clone(&self) -> Self { Self {} }
}

impl ListTasksTool {
    pub fn new() -> Self { Self {} }

    async fn wait_for_task(&self, task_id: String, session_id: String) -> ToolResult {
        // TODO: 需要通过运行时上下文获取 TaskManager
        ToolResult { 
            output: format!("Waiting for task {} in session {} (not yet implemented)", task_id, session_id), 
            error: None 
        }
    }
}

#[async_trait]
impl Tool for ListTasksTool {
    fn name(&self) -> &str { "list_tasks" }
    fn description(&self) -> &str { "获取任务列表。支持立即返回或阻塞等待任务完成。" }
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "task_id": { "type": ["string", "null"], "description": "可选，指定要等待的任务ID" },
                "wait_for_completion": { "type": "boolean", "description": "是否阻塞等待任务完成", "default": false },
                "wait_all": { "type": "boolean", "description": "是否等待当前 spanId 触发的所有任务完成", "default": false }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        // TODO: ListTasksTool 需要通过运行时上下文获取 TaskManager
        // 目前暂时返回占位符信息
        ToolResult { 
            output: "ListTasksTool: TaskManager access needs to be implemented via RuntimeContext".to_string(), 
            error: None 
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> { Box::new(self.clone()) }
}
