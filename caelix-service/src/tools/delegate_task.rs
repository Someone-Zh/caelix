use async_trait::async_trait;
use serde_json::{Value as JsonValue, json};
use std::sync::Arc;

use caelix_api::agent::AgentOutputChunk;
use caelix_api::context::RuntimeContext;
use caelix_api::provider::{ChatMessage, LlmConfig};
use caelix_api::tool::{Tool, ToolResult};
use caelix_runtime::context::CaelixContext;
use caelix_task::{Runnable, TaskKind};

/// 统一执行 Agent 并消费流（纯核心逻辑，无链路ID污染）
async fn run_agent_stream(
    agent: Arc<dyn caelix_api::agent::Agent>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
) -> Result<String, anyhow::Error> {
    use futures::StreamExt;
    let stream = agent.run(messages, provider, config).await;
    let mut result_content = String::new();
    let mut stream = stream;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        // 收集最终结果
        if let AgentOutputChunk::Content { content } = &chunk {
            result_content.push_str(&content);
        }
    }

    Ok(result_content)
}

// ------------------------------
// 委托任务工具（核心主体）
// ------------------------------

#[derive(Clone, Debug)]
pub struct DelegateTaskTool {
    ctx: Arc<CaelixContext>,
}

impl DelegateTaskTool {
    pub fn new(ctx: Arc<CaelixContext>) -> Self {
        Self { ctx }
    }

    /// 统一：获取 Agent + Provider + Config
    async fn prepare_agent_exec(
        &self,
        agent_name: &str,
    ) -> Result<
        (
            Arc<dyn caelix_api::agent::Agent>,
            Arc<dyn caelix_api::provider::LlmProvider>,
            LlmConfig,
        ),
        String,
    > {
        // 1. 获取 Agent
        let agent = self
            .ctx
            .agent_manager
            .get(agent_name)
            .await
            .ok_or_else(|| format!("未找到 agent: {}", agent_name))?;

        // 2. 获取 Provider
        let provider_name = RuntimeContext::try_current()
            .map(|c| c.get_provider().to_string())
            .expect(&format!(
                        "[{}:{}] prepare_agent_exec 没有提供提供者",
                        file!(),
                        line!(),
                    ).to_string());

        let provider_mgr = self.ctx.llm_provider_manager.read().await;
        let all_providers = provider_mgr.get_all_providers();
        let provider = provider_mgr
            .get_provider(&provider_name)
            .cloned()
            .or_else(|| all_providers.first().map(|(_, p)| p.clone()))
            .ok_or("无可用 LLM Provider")?;

        // 3. 获取 Model Config
        let mut model_name = RuntimeContext::try_current()
            .map(|c| c.get_model().to_string())
            .unwrap_or_else(|| provider.config().default_model().to_string());
        if model_name.is_empty() {
            model_name = provider.config().default_model().to_string();
        }

        let config = LlmConfig { model_name };
        Ok((agent, provider, config))
    }

    /// 执行目标 Agent（同步）
    pub async fn execute_agent(
        &self,
        agent_name: &str,
        task_content: &str,
    ) -> Result<String, String> {
        let (agent, provider, config) = self.prepare_agent_exec(agent_name).await?;
        let messages = vec![ChatMessage::user(task_content)];

        run_agent_stream(agent, messages, provider, &config)
            .await
            .map_err(|e| format!("Agent 执行失败: {}", e))
    }
}

// ------------------------------
// 异步任务包装器（极简）
// ------------------------------

#[derive(Clone, Debug)]
struct DelegateTaskRunnable {
    tool: DelegateTaskTool,
    agent_name: String,
    task_content: String,
}

#[async_trait]
impl Runnable for DelegateTaskRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        self.tool
            .execute_agent(&self.agent_name, &self.task_content)
            .await
            .map_err(|e| caelix_api::error::AgentError::ToolError(format!("delegate_task: {e}")))
    }

    fn task_type(&self) -> &'static str {
        "delegate_task"
    }

    fn payload(&self) -> String {
        json!({
            "agent_name": self.agent_name,
            "task_content": self.task_content,
        })
        .to_string()
    }
}

// ------------------------------
// Tool 接口实现（清爽）
// ------------------------------

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "委派任务给指定 Agent 执行"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_name": { "type": "string", "description": "目标 Agent 名称" },
                "task_name": { "type": "string", "description": "任务名称" },
                "task_content": { "type": "string", "description": "任务内容" },
                "sync": { "type": "boolean", "description": "是否同步执行", "default": true }
            },
            "required": ["agent_name", "task_name", "task_content"]
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 参数解析
        let agent_name = match input.get("agent_name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少 agent_name".into()),
                };
            }
        };
        let task_content = match input.get("task_content").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少 task_content".into()),
                };
            }
        };
        let sync = input.get("sync").and_then(|v| v.as_bool()).unwrap_or(true);

        if sync {
            // 同步执行
            match self.execute_agent(agent_name, task_content).await {
                Ok(res) => ToolResult {
                    output: res,
                    error: None,
                },
                Err(error) => ToolResult {
                    output: String::new(),
                    error: Some(error),
                },
            }
        } else {
            // 异步提交任务
            let Some(task_mgr) = &self.ctx.task_manager else {
                return ToolResult::fail("TaskManager 未初始化");
            };

            let runnable = DelegateTaskRunnable {
                tool: self.clone(),
                agent_name: agent_name.to_string(),
                task_content: task_content.to_string(),
            };

            let task_id = task_mgr
                .submit(
                    None,
                    input
                        .get("task_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    TaskKind::Async,
                    Box::new(runnable),
                )
                .await;

            ToolResult::ok(format!("任务已提交，ID: {}", task_id))
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

// 方便转换的小扩展
trait ToolResultExt {
    fn ok(output: impl Into<String>) -> Self;
    fn fail(error: impl Into<String>) -> Self;
}

impl ToolResultExt for ToolResult {
    fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            error: None,
        }
    }
    fn fail(error: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            error: Some(error.into()),
        }
    }
}
