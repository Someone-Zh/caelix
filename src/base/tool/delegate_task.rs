use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

use crate::base::tool::{ToolResult, Tool};
use crate::base::agent::execute_agent_with_messaging;
use crate::base::provider::ChatMessage;
use crate::base::LlmConfig;
use crate::runtime::{RuntimeContext, TaskKind, Runnable};
use crate::runtime::context::RuntimeContextSnapshot;

/// 委派任务工具
/// 允许一个 agent 委派任务给另一个 agent 执行
pub struct DelegateTaskTool;

impl std::fmt::Debug for DelegateTaskTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateTaskTool").finish()
    }
}

impl Clone for DelegateTaskTool {
    fn clone(&self) -> Self {
        Self {}
    }
}

impl DelegateTaskTool {
    pub fn new() -> Self {
        Self {}
    }

    /// 公共方法：执行 agent 任务
    /// 从 RuntimeContext 获取所需的管理器，执行 agent 并仅返回 Content 内容
    async fn execute_agent_task(&self, agent_name: &str, task_content: &str) -> (String, Vec<String>) {
        // 从 RuntimeContext 获取 CaelixContext
        let context = match std::panic::catch_unwind(RuntimeContext::caelix_context) {
            Ok(ctx) => ctx,
            Err(_) => {
                return (String::new(), vec!["无法获取运行时上下文".into()]);
            }
        };
        
        // 获取当前 session_id（用于消息总线）
        let session_id = match std::panic::catch_unwind(RuntimeContext::session_id) {
            Ok(id) => id,
            Err(_) => {
                return (String::new(), vec!["无法获取会话ID".into()]);
            }
        };
        let request_id = crate::runtime::id_generator::generate_request_id();
        let span_id = match std::panic::catch_unwind(RuntimeContext::span_id) {
            Ok(id) => id,
            Err(_) => {
                return (String::new(), vec!["无法获取 Span ID".into()]);
            }
        };
        
        // 从上下文获取 agent
        let agent_spec = match context.agent_manager.get(agent_name).await {
            Some(agent) => agent,
            None => {
                // 获取所有可用的 agent 名称
                let available_agents = context.agent_manager.list_all_names().await;
                let error_msg = format!(
                    "未找到名为 '{}' 的 agent\n\n可用的 agents: {:?}\n\n请检查 agent 名称是否正确，建议使用英文名（如 'collector_agent' 而不是 '收集专家'）",
                    agent_name, available_agents
                );
                return (String::new(), vec![error_msg]);
            }
        };

        // 获取 provider - 优先使用当前上下文的 provider，否则使用默认值
        let provider_name = if let Ok(name) = std::panic::catch_unwind(RuntimeContext::provider) {
            name
        } else {
            // 回退到默认 provider
            context.default_provider.clone()
        };
        let provider_manager = context.llm_provider_manager.read().await;
        let provider = match provider_manager.get_provider(&provider_name) {
            Some(p) => p.clone(),
            None => {
                // 降级：尝试获取第一个可用的 provider
                match provider_manager.get_all_providers().first() {
                    Some((_, p)) => p.clone(),
                    None => {
                        return (String::new(), vec!["未找到可用的 LLM provider".into()]);
                    }
                }
            }
        };

        // 构建消息
        let messages = vec![
            ChatMessage::user(task_content.to_string()),
        ];

        // 配置 - 优先使用当前上下文的 model，否则使用默认值
        let model_name = if let Ok(name) = std::panic::catch_unwind(RuntimeContext::model) {
            name
        } else {
            // 回退到默认 model
            context.default_model.clone()
        };
        let config = LlmConfig {
            model_name,
        };

        // ✅ 使用公共执行器（会自动发送流到消息总线）
        match execute_agent_with_messaging(
            agent_spec,
            messages,
            provider,
            &config,
            session_id,
            request_id,
            span_id,
            Some(agent_name.to_string()),
        ).await {
            Ok(content) => (content, Vec::new()),
            Err(e) => (String::new(), vec![format!("执行 agent 失败：{:?}", e)]),
        }
    }

    /// 同步执行委派任务
    async fn execute_sync(&self, agent_name: &str, _task_name: &str, task_content: &str) -> ToolResult {
        let (result_content, errors) = self.execute_agent_task(agent_name, task_content).await;

        // 如果有错误，返回错误信息
        if !errors.is_empty() {
            return ToolResult {
                output: result_content,
                error: Some(format!("执行过程中出现错误：{}", errors.join("; "))),
            };
        }

        ToolResult {
            output: result_content,
            error: None,
        }
    }

    /// 异步执行委派任务
    async fn execute_async(&self, agent_name: &str, task_name: &str, task_content: &str) -> ToolResult {
        // 从 RuntimeContext 获取 CaelixContext 和 task_manager
        let context = match std::panic::catch_unwind(RuntimeContext::caelix_context) {
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
                    error: Some("异步执行需要配置 TaskManager".into()),
                };
            }
        };

        // 生成 session_id 和 span_id
        // 使用当前上下文的 session_id，确保消息能被正确订阅
        let session_id = match std::panic::catch_unwind(RuntimeContext::session_id) {
            Ok(id) => id,
            Err(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("无法获取会话ID".into()),
                };
            }
        };
        let span_id = crate::runtime::id_generator::generate_span_id();

        // 创建可运行任务
        // 捕获当前 RuntimeContext 快照，用于异步执行时恢复上下文
        let snapshot = RuntimeContextSnapshot::try_from_current();
        
        let runnable = Box::new(DelegateTaskRunnable {
            agent_name: agent_name.to_string(),
            task_name: task_name.to_string(),
            task_content: task_content.to_string(),
            session_id: session_id.clone(),
            span_id: span_id.clone(),
            caelix_context: context.clone(), // 存储 CaelixContext 引用
            runtime_context_snapshot: snapshot,
        });

        // 提交任务到任务管理器
        let task_id = task_manager
            .submit(
                session_id.clone(),
                span_id.clone(),
                None,
                Some(task_name.to_string()),
                TaskKind::Async,
                runnable,
            )
            .await;

        // 返回任务 ID
        ToolResult {
            output: format!("任务已提交，任务ID: {}", task_id),
            error: None,
        }
    }
}

/// 委派任务的可运行包装器
#[derive(Clone)]
struct DelegateTaskRunnable {
    agent_name: String,
    task_name: String,
    task_content: String,
    session_id: String,
    span_id: String,
    caelix_context: Arc<crate::config::CaelixContext>, // 存储 CaelixContext 引用
    // 新增：存储 RuntimeContext 快照，避免依赖 task-local storage
    runtime_context_snapshot: Option<RuntimeContextSnapshot>,
}

impl std::fmt::Debug for DelegateTaskRunnable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateTaskRunnable")
            .field("agent_name", &self.agent_name)
            .field("task_content", &self.task_content)
            .field("has_snapshot", &self.runtime_context_snapshot.is_some())
            .finish()
    }
}

impl DelegateTaskRunnable {
    /// 内部执行方法 - 在 RuntimeContext scope 中调用
    async fn execute_agent_task_inner(&self) -> anyhow::Result<()> {
        // 从 RuntimeContext 获取 CaelixContext
        let context = match std::panic::catch_unwind(RuntimeContext::caelix_context) {
            Ok(ctx) => ctx,
            Err(_) => {
                return Err(anyhow::anyhow!("无法获取运行时上下文"));
            }
        };
        
        // 获取当前 session_id（用于消息总线）
        let session_id = match std::panic::catch_unwind(RuntimeContext::session_id) {
            Ok(id) => id,
            Err(_) => {
                return Err(anyhow::anyhow!("无法获取会话ID"));
            }
        };
        let request_id = crate::runtime::id_generator::generate_request_id();
        let span_id = match std::panic::catch_unwind(RuntimeContext::span_id) {
            Ok(id) => id,
            Err(_) => {
                return Err(anyhow::anyhow!("无法获取 Span ID"));
            }
        };
        
        // 从上下文获取 agent
        let agent_spec = match context.agent_manager.get(&self.agent_name).await {
            Some(agent) => agent,
            None => {
                // 获取所有可用的 agent 名称
                let available_agents = context.agent_manager.list_all_names().await;
                return Err(anyhow::anyhow!(
                    "未找到名为 '{}' 的 agent\n\n可用的 agents: {:?}\n\n请检查 agent 名称是否正确，建议使用英文名（如 'collector_agent' 而不是 '收集专家'）",
                    self.agent_name, available_agents
                ));
            }
        };

        // 获取 provider - 优先使用快照中的 provider，否则使用默认值
        let provider_name = if let Some(snapshot) = &self.runtime_context_snapshot {
            snapshot.provider.clone()
        } else {
            // 回退到默认 provider
            context.default_provider.clone()
        };
        
        let provider_manager = context.llm_provider_manager.read().await;
        let provider = match provider_manager.get_provider(&provider_name) {
            Some(p) => p.clone(),
            None => {
                // 降级：尝试获取第一个可用的 provider
                match provider_manager.get_all_providers().first() {
                    Some((_, p)) => p.clone(),
                    None => {
                        return Err(anyhow::anyhow!("未找到可用的 LLM provider"));
                    }
                }
            }
        };

        // 构建消息
        let messages = vec![
            ChatMessage::user(self.task_content.clone()),
        ];

        // 配置 - 优先使用快照中的 model，否则使用默认值
        let model_name = if let Some(snapshot) = &self.runtime_context_snapshot {
            snapshot.model.clone()
        } else {
            // 回退到默认 model
            context.default_model.clone()
        };
        
        let config = LlmConfig {
            model_name,
        };

        // ✅ 使用公共执行器（会自动发送流到消息总线）
        let _result = execute_agent_with_messaging(
            agent_spec,
            messages,
            provider.clone(),
            &config,
            session_id.clone(),
            request_id,
            span_id.clone(),
            Some(self.agent_name.clone()),
        ).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Runnable for DelegateTaskRunnable {
    async fn run(&self) -> anyhow::Result<()> {
        // 如果有快照，则重建 RuntimeContext 并在其 scope 中执行
        if let Some(snapshot) = &self.runtime_context_snapshot {
            // 需要重建完整的 RuntimeContext
            let caelix_ctx = self.caelix_context.clone();
            let work_dir = snapshot.work_dir.clone();
            
            let runtime_ctx = crate::runtime::context::RuntimeContext::new(
                Some(self.session_id.clone()),
                Some(crate::runtime::id_generator::generate_request_id()),
                work_dir,
                snapshot.provider.clone(),
                snapshot.model.clone(),
                snapshot.debug_enabled,
                caelix_ctx,
            );
            
            // 在 RuntimeContext scope 中执行
            return crate::runtime::context::RuntimeContext::scope(runtime_ctx, async {
                // 执行 agent 任务
                self.execute_agent_task_inner().await
            }).await;
        } else {
            // 没有快照的情况下，记录警告并尝试执行
            eprintln!("Warning: No RuntimeContext snapshot available for delegate task {}", self.task_name);
            // 仍然尝试执行，但可能会失败
            self.execute_agent_task_inner().await
        }
    }

    fn task_type(&self) -> &'static str {
        "delegate_task"
    }

    fn payload(&self) -> String {
        serde_json::json!({
            "agent_name": self.agent_name,
            "task_name": self.task_name,
            "task_content": self.task_content,
            "session_id": self.session_id,
            "span_id": self.span_id,
        }).to_string()
    }
}

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "委派任务给指定的 agent 执行。需要提供目标 agent 的名称和任务内容。"
    }

    /// JSON 参数 schema：agent_name(必选), task_name(必选), task_content(必选), sync(可选，默认true)
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_name": {
                    "type": "string",
                    "description": "要委派任务的 agent 名称"
                },
                "task_name": {
                    "type": "string",
                    "description": "任务名称，用于提高任务可读性"
                },
                "task_content": {
                    "type": "string",
                    "description": "要执行的任务内容描述"
                },
                "sync": {
                    "type": "boolean",
                    "description": "是否同步执行，true=等待结果返回，false=异步执行并返回任务ID，默认true"
                }
            },
            "required": ["agent_name", "task_name", "task_content"]
        })
    }

    /// 执行委派任务
    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 解析参数
        let agent_name = match input.get("agent_name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少参数：agent_name".into()),
                };
            }
        };

        let task_name = match input.get("task_name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少参数：task_name".into()),
                };
            }
        };

        let task_content = match input.get("task_content").and_then(|v| v.as_str()) {
            Some(content) => content.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少参数：task_content".into()),
                };
            }
        };

        // 解析 sync 参数，默认为 true（同步）
        let is_sync = input
            .get("sync")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if is_sync {
            // 同步模式：直接执行并返回结果
            self.execute_sync(&agent_name, &task_name, &task_content).await
        } else {
            // 异步模式：提交到任务队列
            self.execute_async(&agent_name, &task_name, &task_content).await
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
