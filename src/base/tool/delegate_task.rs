use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use futures::StreamExt;

use crate::base::tool::{ToolResult, Tool};
use crate::base::agent::Agent;
use crate::base::provider::ChatMessage;
use crate::base::LlmConfig;
use crate::runtime::{RuntimeContext, TaskKind, Runnable};

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
        let context = RuntimeContext::caelix_context();
        
        // 从上下文获取 agent
        let agent_spec = match context.agent_manager.get(agent_name).await {
            Some(agent) => agent,
            None => {
                return (String::new(), vec![format!("未找到名为 '{}' 的 agent", agent_name)]);
            }
        };

        // 获取 provider
        let provider_manager = context.llm_provider_manager.read().await;
        let provider = match provider_manager.get_provider("bailian") {
            Some(p) => p,
            None => {
                return (String::new(), vec!["未找到默认的 LLM provider".into()]);
            }
        };

        // 构建消息
        let messages = vec![
            ChatMessage::user(task_content.to_string()),
        ];

        // 配置
        let config = LlmConfig {
            model_name: provider.config().default_model().to_string(),
        };

        // 执行 agent
        let mut stream = match agent_spec.execute(messages, provider.clone(), &config).await {
            Ok(stream) => stream,
            Err(e) => {
                return (String::new(), vec![format!("执行 agent 失败：{:?}", e)]);
            }
        };

        // 收集结果 - 仅保留 Content 类型
        let mut result_content = String::new();
        let mut errors = Vec::new();
        
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    use crate::base::agent::AgentOutputChunk;
                    match chunk {
                        AgentOutputChunk::Content { content } => {
                            result_content.push_str(&content);
                        }
                        // 忽略 Reasoning、ToolCall、ToolResult、Finish
                        _ => {}
                    }
                }
                Err(e) => {
                    errors.push(format!("{:?}", e));
                }
            }
        }

        (result_content, errors)
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
        let context = RuntimeContext::caelix_context();
        
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
        let session_id = format!("delegate_{}", agent_name);
        let span_id = crate::runtime::Message::generate_span_id();

        // 创建可运行任务
        let runnable = Box::new(DelegateTaskRunnable {
            agent_name: agent_name.to_string(),
            task_name: task_name.to_string(),
            task_content: task_content.to_string(),
            session_id: session_id.clone(),
            span_id: span_id.clone(),
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
}

impl std::fmt::Debug for DelegateTaskRunnable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateTaskRunnable")
            .field("agent_name", &self.agent_name)
            .field("task_content", &self.task_content)
            .finish()
    }
}

#[async_trait::async_trait]
impl Runnable for DelegateTaskRunnable {
    async fn run(&self) -> anyhow::Result<()> {
        // 从 RuntimeContext 获取 CaelixContext
        let context = RuntimeContext::caelix_context();
        
        // 从上下文获取 agent
        let agent_spec = match context.agent_manager.get(&self.agent_name).await {
            Some(agent) => agent,
            None => {
                return Err(anyhow::anyhow!("未找到名为 '{}' 的 agent", self.agent_name));
            }
        };

        // 获取 provider
        let provider_manager = context.llm_provider_manager.read().await;
        let provider = match provider_manager.get_provider("bailian") {
            Some(p) => p,
            None => {
                return Err(anyhow::anyhow!("未找到默认的 LLM provider"));
            }
        };

        // 构建消息
        let messages = vec![
            ChatMessage::user(self.task_content.clone()),
        ];

        // 配置
        let config = LlmConfig {
            model_name: provider.config().default_model().to_string(),
        };

        // 执行 agent
        let mut stream = match agent_spec.execute(messages, provider.clone(), &config).await {
            Ok(stream) => stream,
            Err(e) => {
                return Err(anyhow::anyhow!("执行 agent 失败：{:?}", e));
            }
        };

        // 收集结果 - 仅保留 Content 类型
        let mut result_content = String::new();
        
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    use crate::base::agent::AgentOutputChunk;
                    match chunk {
                        AgentOutputChunk::Content { content } => {
                            result_content.push_str(&content);
                        }
                        // 忽略 Reasoning、ToolCall、ToolResult、Finish
                        _ => {}
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("执行过程中出现错误：{:?}", e));
                }
            }
        }

        Ok(())
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
