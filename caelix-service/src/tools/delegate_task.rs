use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

use caelix_api::tool::{ToolResult, Tool};
use caelix_api::agent::AgentOutputChunk;
use caelix_api::message::{AgentMessage, AgentMessageType};
use caelix_runtime::context::{RuntimeContext, RuntimeContextSnapshot};
use caelix_api::provider::{ChatMessage, LlmConfig};
use caelix_task::{Runnable, TaskKind};
use crate::context::CaelixContext;

/// 临时执行器：执行 agent 并将流发送到消息总线
/// TODO: 后续会通过别的方式实现
#[allow(clippy::too_many_arguments)]
async fn execute_agent_with_messaging_local(
    agent_spec: Arc<caelix_api::agent::AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
    session_id: String,
    request_id: String,
    span_id: String,
    agent_name: Option<String>,
    message_bus: Arc<caelix_message::MessageBus>,
) -> Result<String, anyhow::Error> {
    use futures::StreamExt;
    
    // 使用 loop_runner 执行 agent
    let stream = caelix_agent::loop_runner::run_agent_loop(
        (*agent_spec).clone(),
        messages,
        provider,
        config.clone(),
    ).await?;
    
    let mut result_content = String::new();
    let mut stream = stream;
    
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // 提取内容并积累
                let content = extract_chunk_content(&chunk);
                if !content.is_empty() {
                    result_content.push_str(&content);
                }
                
                // 发送 Chunk 到消息总线
                let chunk_msg = AgentMessage {
                    session_id: session_id.clone(),
                    request_id: request_id.clone(),
                    span_id: span_id.clone(),
                    r#type: AgentMessageType::Chunk,
                    timestamp: chrono::Utc::now(),
                    content: content.clone(),
                    agent_name: agent_name.clone(),
                };
                let _ = message_bus.send_agent(chunk_msg);
                
                // 如果是 Finish，发送 ChunkEnd
                if matches!(chunk, AgentOutputChunk::Finish { .. }) {
                    let end_msg = AgentMessage {
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        span_id: span_id.clone(),
                        r#type: AgentMessageType::ChunkEnd,
                        timestamp: chrono::Utc::now(),
                        content: String::new(),
                        agent_name: agent_name.clone(),
                    };
                    let _ = message_bus.send_agent(end_msg);
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Agent execution error: {:?}", e));
            }
        }
    }
    
    Ok(result_content)
}

/// 从 AgentOutputChunk 提取文本内容
fn extract_chunk_content(chunk: &AgentOutputChunk) -> String {
    match chunk {
        AgentOutputChunk::Content { content } => content.clone(),
        AgentOutputChunk::Reasoning { content } => content.clone(),
        AgentOutputChunk::ToolCall { name, arguments, .. } => {
            format!("\n[工具调用] {}({})", name, arguments)
        }
        AgentOutputChunk::ToolResult { tool_name, result } => {
            format!("\n[工具结果] {}: {}", tool_name, result)
        }
        AgentOutputChunk::Start { timestamp } => {
            format!("\n[开始] {}", timestamp.format("%H:%M:%S"))
        },
        AgentOutputChunk::CallProvider { timestamp, provider, model } => {
            format!("\n[调用模型] {} {}@{}", timestamp.format("%H:%M:%S"), provider, model)
        },
        AgentOutputChunk::Finish { .. } => String::new(),
    }
}

/// 委派任务工具
pub struct DelegateTaskTool {
    caelix_context: Arc<CaelixContext>,
}

impl std::fmt::Debug for DelegateTaskTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateTaskTool").finish()
    }
}

impl Clone for DelegateTaskTool {
    fn clone(&self) -> Self {
        Self {
            caelix_context: self.caelix_context.clone(),
        }
    }
}

impl DelegateTaskTool {
    pub fn new(caelix_context: Arc<CaelixContext>) -> Self {
        Self { caelix_context }
    }

    /// 执行 agent 任务
    async fn execute_agent_task(&self, agent_name: &str, task_content: &str) -> Result<String, String> {
        let context = &self.caelix_context;
        
        // 获取当前 trace_id（保持链路一致，为将来扩展预留）
        let _trace_id = match std::panic::catch_unwind(|| RuntimeContext::trace_id()) {
            Ok(id) => id,
            Err(_) => caelix_api::utils::generate_trace_id(),
        };
        
        // 为委派任务创建新的 session_id、request_id 和 span_id
        let new_session_id = caelix_api::utils::generate_session_id();
        let request_id = caelix_api::utils::generate_request_id();
        let span_id = caelix_api::utils::generate_span_id();
        
        // 从上下文获取 agent
        let agent_spec = context.agent_manager.get(agent_name).await
            .ok_or_else(|| {
                format!("未找到 agent '{}'", agent_name)
            })?;

        // 获取 provider
        let provider_name = match std::panic::catch_unwind(|| RuntimeContext::provider()) {
            Ok(name) => name,
            Err(_) => context.default_provider.clone(),
        };
        
        let provider_manager = context.llm_provider_manager.read().await;
        let provider = provider_manager.get_provider(&provider_name)
            .cloned()
            .or_else(|| {
                provider_manager.get_all_providers().first().map(|(_, p)| p.clone())
            })
            .ok_or_else(|| "未找到可用的 LLM provider".to_string())?;

        // 构建消息
        let messages = vec![ChatMessage::user(task_content.to_string())];

        // 配置
        let model_name = match std::panic::catch_unwind(|| RuntimeContext::model()) {
            Ok(name) => name,
            Err(_) => context.default_model.clone(),
        };
        let config = LlmConfig { model_name };

        // 执行 agent（使用新的 session_id）
        execute_agent_with_messaging_local(
            agent_spec,
            messages,
            provider,
            &config,
            new_session_id,  // 新的 session，隔离对话历史
            request_id,
            span_id,
            Some(agent_name.to_string()),
            context.message_bus.clone(),
        ).await.map_err(|e| format!("执行失败: {:?}", e))
    }

    /// 同步执行
    async fn execute_sync(&self, agent_name: &str, task_content: &str) -> ToolResult {
        match self.execute_agent_task(agent_name, task_content).await {
            Ok(content) => ToolResult { output: content, error: None },
            Err(error) => ToolResult { output: String::new(), error: Some(error) },
        }
    }

    /// 异步执行
    async fn execute_async(&self, agent_name: &str, task_name: &str, task_content: &str) -> ToolResult {
        let task_manager = match &self.caelix_context.task_manager {
            Some(tm) => tm,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("TaskManager 未初始化".into()),
                };
            }
        };

        // 获取当前 session_id 和 trace_id
        let session_id = match std::panic::catch_unwind(|| RuntimeContext::session_id()) {
            Ok(id) => id,
            Err(_) => "unknown".to_string(),
        };
        let trace_id = match std::panic::catch_unwind(|| RuntimeContext::trace_id()) {
            Ok(id) => id,
            Err(_) => caelix_api::utils::generate_trace_id(),
        };
        let span_id = caelix_api::utils::generate_span_id();

        let snapshot = RuntimeContextSnapshot::try_from_current();
        
        let runnable = Box::new(DelegateTaskRunnable {
            agent_name: agent_name.to_string(),
            task_name: task_name.to_string(),
            task_content: task_content.to_string(),
            session_id: session_id.clone(),
            span_id: span_id.clone(),
            trace_id: trace_id.clone(),
            caelix_context: self.caelix_context.clone(),
            runtime_context_snapshot: snapshot,
        });

        let task_id = task_manager.submit(
            session_id,
            span_id,
            None,
            Some(task_name.to_string()),
            TaskKind::Async,
            runnable,
        ).await;

        ToolResult {
            output: format!("任务已提交，ID: {}", task_id),
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
    #[allow(dead_code)] // 为将来扩展预留
    session_id: String,
    #[allow(dead_code)] // 为将来扩展预留
    span_id: String,
    #[allow(dead_code)] // 为将来扩展预留
    trace_id: String,
    caelix_context: Arc<CaelixContext>,
    #[allow(dead_code)] // 为将来扩展预留
    runtime_context_snapshot: Option<RuntimeContextSnapshot>,
}

impl std::fmt::Debug for DelegateTaskRunnable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateTaskRunnable")
            .field("agent_name", &self.agent_name)
            .finish()
    }
}

#[async_trait::async_trait]
impl Runnable for DelegateTaskRunnable {
    async fn run(&self) -> anyhow::Result<()> {
        let tool = DelegateTaskTool::new(self.caelix_context.clone());
        tool.execute_agent_task(&self.agent_name, &self.task_content).await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn task_type(&self) -> &'static str {
        "delegate_task"
    }

    fn payload(&self) -> String {
        json!({
            "agent_name": self.agent_name,
            "task_name": self.task_name,
            "task_content": self.task_content,
        }).to_string()
    }
}

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str { "delegate_task" }
    fn description(&self) -> &str { "委派任务给指定的 agent 执行" }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "agent_name": { "type": "string", "description": "目标 agent 名称" },
                "task_name": { "type": "string", "description": "任务名称" },
                "task_content": { "type": "string", "description": "任务内容" },
                "sync": { "type": "boolean", "description": "是否同步执行", "default": true }
            },
            "required": ["agent_name", "task_name", "task_content"]
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let agent_name = match input.get("agent_name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return ToolResult { output: String::new(), error: Some("缺少 agent_name".into()) },
        };
        let task_name = match input.get("task_name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return ToolResult { output: String::new(), error: Some("缺少 task_name".into()) },
        };
        let task_content = match input.get("task_content").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => return ToolResult { output: String::new(), error: Some("缺少 task_content".into()) },
        };
        let is_sync = input.get("sync").and_then(|v| v.as_bool()).unwrap_or(true);

        if is_sync {
            self.execute_sync(&agent_name, &task_content).await
        } else {
            self.execute_async(&agent_name, &task_name, &task_content).await
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
