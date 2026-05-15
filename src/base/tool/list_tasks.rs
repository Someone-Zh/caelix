use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;

use crate::base::tool::{ToolResult, Tool};
use crate::runtime::{RuntimeContext, task::types::TaskStatus};
use crate::runtime::message::task_message::TaskMessageType;

/// 任务列表工具
/// 支持立即返回、按 taskId 阻塞等待、全部任务阻塞等待
pub struct ListTasksTool;

impl std::fmt::Debug for ListTasksTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListTasksTool").finish()
    }
}

impl Clone for ListTasksTool {
    fn clone(&self) -> Self {
        Self {}
    }
}

impl ListTasksTool {
    pub fn new() -> Self {
        Self {}
    }

    /// 阻塞等待指定任务完成
    async fn wait_for_task(&self, task_id: String, session_id: String) -> ToolResult {
        let context = RuntimeContext::caelix_context();
        
        // 检查是否有 task_manager
        let task_manager = match &context.task_manager {
            Some(tm) => tm,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("TaskManager not initialized".into()),
                };
            }
        };

        // 订阅任务消息
        let mut task_receiver = context.message_bus.subscribe_task();
        
        // 等待任务完成
        loop {
            match task_receiver.recv().await {
                Ok(msg) => {
                    if msg.session_id == session_id {
                        // 从 content 中提取 task_id (格式: "Task {task_id} completed/failed")
                        if msg.content.contains(&task_id) {
                            if matches!(msg.r#type, TaskMessageType::Completed | TaskMessageType::Failed) {
                                // 获取最终任务状态
                                if let Some(meta) = task_manager.get_status(&crate::runtime::TaskId(task_id.clone())).await {
                                    return ToolResult {
                                        output: serde_json::to_string_pretty(&meta).unwrap_or_default(),
                                        error: None,
                                    };
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    return ToolResult {
                        output: String::new(),
                        error: Some("Failed to receive task completion message".into()),
                    };
                }
            }
        }
    }

    /// 阻塞等待当前 span_id 触发的所有任务完成
    async fn wait_for_all_tasks(&self, session_id: String) -> ToolResult {
        let context = match std::panic::catch_unwind(|| RuntimeContext::caelix_context()) {
            Ok(ctx) => ctx,
            Err(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("无法获取运行时上下文".into()),
                };
            }
        };
        let current_span_id = match std::panic::catch_unwind(|| RuntimeContext::span_id()) {
            Ok(id) => id,
            Err(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("无法获取 Span ID".into()),
                };
            }
        };
        
        // 检查是否有 task_manager
        let task_manager = match &context.task_manager {
            Some(tm) => tm,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("TaskManager not initialized".into()),
                };
            }
        };

        // 获取当前 span_id 的所有未完成任务
        let all_tasks = task_manager.list_tasks(Some(&session_id)).await;
        let tasks_to_wait: HashSet<String> = all_tasks.iter()
            .filter(|t| {
                t.span_id == current_span_id && 
                !matches!(t.status, TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Cancelled)
            })
            .map(|t| t.task_id.0.clone())
            .collect();

        if tasks_to_wait.is_empty() {
            return ToolResult {
                output: serde_json::to_string_pretty(&all_tasks).unwrap_or_default(),
                error: None,
            };
        }

        // 订阅任务消息
        let mut task_receiver = context.message_bus.subscribe_task();
        let mut completed_tasks = HashSet::new();

        // 等待所有任务完成
        while completed_tasks.len() < tasks_to_wait.len() {
            match task_receiver.recv().await {
                Ok(msg) => {
                    if msg.session_id == session_id {
                        // 从 content 中提取 task_id
                        for task_id in &tasks_to_wait {
                            if msg.content.contains(task_id) && !completed_tasks.contains(task_id) {
                                if matches!(msg.r#type, TaskMessageType::Completed | TaskMessageType::Failed) {
                                    completed_tasks.insert(task_id.clone());
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    return ToolResult {
                        output: String::new(),
                        error: Some("Failed to receive task completion messages".into()),
                    };
                }
            }
        }

        // 获取最终任务列表
        let final_tasks = task_manager.list_tasks(Some(&session_id)).await;
        ToolResult {
            output: serde_json::to_string_pretty(&final_tasks).unwrap_or_default(),
            error: None,
        }
    }
}

#[async_trait]
impl Tool for ListTasksTool {
    fn name(&self) -> &str {
        "list_tasks"
    }

    fn description(&self) -> &str {
        "获取任务列表。支持立即返回、按 taskId 阻塞等待某个任务完成后返回，或等待当前 spanId 触发的所有任务完成。"
    }

    /// JSON 参数 schema
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": ["string", "null"],
                    "description": "可选，指定要等待的任务ID。如果不提供则返回当前spanId触发的所有任务列表"
                },
                "wait_for_completion": {
                    "type": "boolean",
                    "description": "是否阻塞等待任务完成。默认false立即返回",
                    "default": false
                },
                "wait_all": {
                    "type": "boolean",
                    "description": "如果为true且未指定task_id，则等待当前spanId触发的所有任务完成",
                    "default": false
                }
            }
        })
    }

    /// 执行任务列表查询
    async fn execute(&self, input: JsonValue) -> ToolResult {
        let context = match std::panic::catch_unwind(|| RuntimeContext::caelix_context()) {
            Ok(ctx) => ctx,
            Err(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("无法获取运行时上下文".into()),
                };
            }
        };
        
        // 检查是否有 task_manager
        let task_manager = match &context.task_manager {
            Some(tm) => tm,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("TaskManager not initialized".into()),
                };
            }
        };

        // 解析参数
        let task_id = input.get("task_id").and_then(|v| v.as_str()).map(|s| s.to_string());
        let wait_for_completion = input
            .get("wait_for_completion")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let wait_all = input
            .get("wait_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 获取 session_id
        let session_id = match std::panic::catch_unwind(|| RuntimeContext::session_id()) {
            Ok(id) => id,
            Err(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("无法获取会话ID".into()),
                };
            }
        };

        if !wait_for_completion {
            // 立即返回任务列表
            let tasks = task_manager.list_tasks(Some(&session_id)).await;
            ToolResult {
                output: serde_json::to_string_pretty(&tasks).unwrap_or_default(),
                error: None,
            }
        } else if let Some(tid) = task_id {
            // 阻塞等待指定任务完成
            self.wait_for_task(tid, session_id).await
        } else if wait_all {
            // 阻塞等待所有任务完成
            self.wait_for_all_tasks(session_id).await
        } else {
            // wait_for_completion=true 但没有指定 task_id 且 wait_all=false
            // 默认行为：立即返回
            let tasks = task_manager.list_tasks(Some(&session_id)).await;
            ToolResult {
                output: serde_json::to_string_pretty(&tasks).unwrap_or_default(),
                error: None,
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
