use crate::api_trait::CaelixApi;
use crate::types::{ChatAsyncResult, ChatRequest, ProviderInfo, SessionSummary};
use crate::variable_replacer::VariableReplacer;
use async_trait::async_trait;
use caelix_api::context::{ContextFutureExt, RuntimeContext};
use caelix_api::error::ApiError;
use caelix_api::{AgentRunManagerTrait, ContextProvider, EnvConfigTrait};
use caelix_api::message::{AgentMessage, AgentMessageType, NotificationMessage};
use caelix_api::provider::{ChatMessage, GlobalUsageView, LlmConfig, LlmProvider, SessionUsageView};
use caelix_api::task::TaskMeta;
use caelix_api::tool::ToolResult;
use caelix_runtime::context::CaelixContext;
use futures::{Stream, StreamExt, future, stream};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// 会话摘要截取的最大字符数
const SUMMARY_MAX_CHARS: usize = 15;

/// 工具执行超时时间（秒）
const TOOL_EXECUTION_TIMEOUT_SECS: u64 = 300;

/// list_sessions 最大并发数
const LIST_SESSIONS_MAX_CONCURRENCY: usize = 10;

/// 校验 session_id：非空且仅允许 [A-Za-z0-9_-]
///
/// 与 FileStorage / FileTaskStorage 的校验规则一致，防止路径穿越。
fn validate_session_id(session_id: &str) -> Result<(), ApiError> {
    if !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(ApiError::invalid_request(
            "session_id 非法：仅允许非空的 [A-Za-z0-9_-]",
        ))
    }
}

/// 构造 AgentMessage（审批场景辅助函数，request_id/span_id/trace_id 留空）
fn make_agent_message(
    session_id: &str,
    msg_type: AgentMessageType,
    content: String,
    agent_name: Option<String>,
) -> AgentMessage {
    AgentMessage {
        session_id: session_id.to_string(),
        request_id: String::new(),
        span_id: String::new(),
        trace_id: String::new(),
        r#type: msg_type,
        timestamp: chrono::Utc::now(),
        content,
        agent_name,
        usage: None,
    }
}

/// 从消息列表中提取首条消息作为摘要
///
/// 注意：调用方应尽可能只传入必要的消息（如前 N 条），避免全量加载。
fn summarize_first_message(messages: &[AgentMessage]) -> String {
    messages
        .first()
        .map(|msg| {
            let actual_content =
                serde_json::from_str::<ChatMessage>(&msg.content)
                    .map(|cm| cm.content)
                    .unwrap_or_else(|_| msg.content.clone());

            let mut result = String::with_capacity(SUMMARY_MAX_CHARS + 3);
            for (i, ch) in actual_content.chars().enumerate() {
                if i >= SUMMARY_MAX_CHARS {
                    result.push_str("...");
                    break;
                }
                result.push(ch);
            }
            result
        })
        .unwrap_or_else(|| "新会话".to_string())
}

/// 执行 Agent
async fn run_agent(
    agent_spec: Arc<caelix_api::agent::AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn LlmProvider>,
    config: &LlmConfig,
) -> Result<String, anyhow::Error> {
    caelix_agent::run_agent(agent_spec, messages, provider, config)
        .await
        .map_err(|e| anyhow::anyhow!("Agent execution error: {:?}", e))
}

/// 构造并持久化一条 tool 结果消息，同时发送 Event 通知。
///
/// 这是 `execute_approved_tool` 和 `append_rejection_result` 的公共逻辑抽离。
async fn persist_and_notify_tool_result(
    context: &CaelixContext,
    session_id: &str,
    tool_call_id: &str,
    result_text: String,
    agent_name: Option<String>,
    event_text: String,
) -> Result<(), ApiError> {
    let chat_tool_msg = ChatMessage {
        role: "tool".to_string(),
        content: result_text,
        tool_calls: None,
        tool_call_id: Some(tool_call_id.to_string()),
    };

    let storage = context.session_manager.get_storage();
    let agent_msg = make_agent_message(
        session_id,
        AgentMessageType::Msg,
        serde_json::to_string(&chat_tool_msg).map_err(|e| {
            ApiError::InternalError(format!("序列化 tool result 失败: {}", e))
        })?,
        agent_name.clone(),
    );
    storage
        .append_agent_message(&agent_msg)
        .await
        .map_err(|e| ApiError::InternalError(format!("持久化 tool_result 失败: {}", e)))?;

    let event_msg = make_agent_message(
        session_id,
        AgentMessageType::Event,
        event_text,
        agent_name,
    );
    if context.message_bus.send_agent(event_msg).is_err() {
        tracing::warn!("Failed to send tool result event message to message bus");
    }

    Ok(())
}

/// API 核心实现
pub struct CaelixApiImpl {
    context: Arc<CaelixContext>,
}

impl CaelixApiImpl {
    pub fn new(context: Arc<CaelixContext>) -> Self {
        Self { context }
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<caelix_message::MessageBus> {
        &self.context.message_bus
    }

    /// 获取 SessionManager 引用
    pub fn session_manager(&self) -> &caelix_message::SessionManager {
        &self.context.session_manager
    }

    /// 在 RuntimeContext 作用域内执行工具，带超时保护和取消支持。
    ///
    /// 取消优先级：
    /// 1. 若 session 有正在运行的 Agent，使用 Agent 的 cancel_token 子令牌
    ///    （调用 stop_agent 会级联取消工具执行）
    /// 2. 超时保护（TOOL_EXECUTION_TIMEOUT_SECS）
    async fn execute_tool_with_context(
        &self,
        session_id: &str,
        tool: Arc<dyn caelix_api::tool::Tool>,
        args_json: serde_json::Value,
    ) -> String {
        let session_config = self.context.session_manager.get_session_config(session_id).await;
        let provider_name = session_config
            .as_ref()
            .and_then(|c| c.provider.clone())
            .unwrap_or_default();
        let model_name = session_config
            .as_ref()
            .and_then(|c| c.model.clone())
            .unwrap_or_default();
        let work_dir = std::env::current_dir().unwrap_or_default();

        // 优先复用当前 Agent 运行的 cancel_token（子令牌），确保 stop_agent 能停止工具
        let cancel_token = self
            .context
            .agent_run_manager
            .get_cancel_token(session_id)
            .unwrap_or_else(caelix_api::cancel::CancellationToken::new);

        let runtime_ctx = Arc::new(RuntimeContext::new(
            Some(session_id.to_string()),
            None,
            work_dir,
            provider_name,
            model_name,
            self.context.env_config.debug_enabled(),
            cancel_token.clone(),
        ));

        let ctx_for_scope = runtime_ctx.clone();
        let tool_fut = async move { tool.execute(args_json).await };

        // 使用 tokio::select! 同时监听超时和取消信号
        let cancel_fut = cancel_token.cancelled();
        let timeout_dur = Duration::from_secs(TOOL_EXECUTION_TIMEOUT_SECS);

        tokio::select! {
            result = tool_fut.with_runtime_ctx(ctx_for_scope) => {
                match result {
                    ToolResult {
                        error: Some(e),
                        ..
                    } => format!("[ERROR] {}", e),
                    ToolResult {
                        output,
                        error: None,
                    } => output,
                }
            }
            _ = cancel_fut => {
                format!("[ERROR] Tool execution cancelled")
            }
            _ = tokio::time::sleep(timeout_dur) => {
                format!(
                    "[ERROR] Tool execution timed out ({}s)",
                    TOOL_EXECUTION_TIMEOUT_SECS
                )
            }
        }
    }

    /// 执行已批准的工具调用
    async fn execute_approved_tool(
        &self,
        session_id: &str,
        tool_call_id: &str,
        agent_name: &str,
        chat_msg: &ChatMessage,
        tool_name: &str,
    ) -> Result<(), ApiError> {
        // 通过 ConfigOverlay 获取 agent_spec（优先项目级配置，回退全局）
        let overlay = self.context.config_overlay();
        let agent_spec = overlay
            .get_agent_spec(agent_name)
            .await
            .ok_or_else(|| ApiError::agent_not_found(agent_name))?;

        // 找到对应 tool_call 并执行
        let mut tool_result_text = String::new();
        if let Some(tcs) = &chat_msg.tool_calls {
            for tc in tcs.iter() {
                if tc.id != tool_call_id {
                    continue;
                }

                // 解析参数
                let args_json = match &tc.arguments {
                    serde_json::Value::String(s) => serde_json::from_str::<serde_json::Value>(s)
                        .unwrap_or_else(|_| serde_json::Value::String(s.clone())),
                    other => other.clone(),
                };

                // 查找工具
                match agent_spec.tools.iter().find(|t| t.name() == tc.name) {
                    Some(tool) => {
                        tool_result_text = self
                            .execute_tool_with_context(session_id, tool.clone(), args_json)
                            .await;
                    }
                    None => {
                        tool_result_text = format!("[ERROR] Tool '{}' not found", tc.name);
                    }
                }
                break;
            }
        }

        persist_and_notify_tool_result(
            &self.context,
            session_id,
            tool_call_id,
            tool_result_text,
            Some(agent_name.to_string()),
            format!(
                "[已批准] tool_call_id={}, tool_name={}",
                tool_call_id, tool_name
            ),
        )
        .await
    }

    /// 追加拒绝执行的工具结果消息
    async fn append_rejection_result(
        &self,
        session_id: &str,
        tool_call_id: &str,
        tool_name: &str,
    ) -> Result<(), ApiError> {
        persist_and_notify_tool_result(
            &self.context,
            session_id,
            tool_call_id,
            format!("[REJECTED] tool_call_id={} 已被拒绝执行", tool_call_id),
            None,
            format!(
                "[已拒绝] tool_call_id={}, tool_name={}",
                tool_call_id, tool_name
            ),
        )
        .await
    }

    /// 获取第一个 Provider 的名称（用于默认值回退）
    fn first_provider_name(&self) -> Option<String> {
        self.context
            .llm_provider_manager
            .try_read()
            .ok()
            .and_then(|pm| pm.get_all_providers().first().map(|(n, _)| n.clone()))
    }

    /// 获取第一个 Provider 的默认模型名（用于默认值回退）
    fn first_provider_default_model(&self) -> Option<String> {
        self.context
            .llm_provider_manager
            .try_read()
            .ok()
            .and_then(|pm| {
                pm.get_all_providers()
                    .first()
                    .and_then(|(_, p)| {
                        let config = p.config();
                        config
                            .default_model
                            .clone()
                            .or_else(|| config.models.values().next().cloned())
                    })
            })
    }
}

#[async_trait]
impl CaelixApi for CaelixApiImpl {
    fn get_default_provider(&self) -> Option<String> {
        // 同步方法无法 await RwLock，通过 try_read 尝试获取
        // 若获取失败返回 None，调用方应处理空值情况
        self.first_provider_name()
    }

    fn get_default_model(&self) -> Option<String> {
        self.first_provider_default_model()
    }

    async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<(), ApiError> {
        validate_session_id(session_id)?;

        // 验证提供者是否存在
        let provider_manager = self.context.llm_provider_manager.read().await;
        if provider_manager.get_provider(provider).is_none() {
            return Err(ApiError::provider_not_found(provider));
        }

        self.context
            .session_manager
            .set_session_provider(session_id, provider)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), ApiError> {
        validate_session_id(session_id)?;

        // 校验模型是否存在：
        // 1. 若 session 已绑定 provider，在该 provider 的模型列表中校验
        // 2. 若未绑定 provider，在所有 provider 的模型列表中校验（确保模型合法）
        let session_config = self.context.session_manager.get_session_config(session_id).await;
        let model_valid = match session_config {
            Some(ref config) if config.provider.is_some() => {
                let provider_name = config.provider.as_deref().unwrap();
                let provider_manager = self.context.llm_provider_manager.read().await;
                if let Some(provider) = provider_manager.get_provider(provider_name) {
                    let pconfig = provider.config();
                    pconfig.models.values().any(|m| m == model)
                        || pconfig.default_model.as_deref() == Some(model)
                } else {
                    false
                }
            }
            _ => {
                let provider_manager = self.context.llm_provider_manager.read().await;
                provider_manager.get_all_providers().iter().any(|(_, p)| {
                    let pconfig = p.config();
                    pconfig.models.values().any(|m| m == model)
                        || pconfig.default_model.as_deref() == Some(model)
                })
            }
        };

        if !model_valid {
            return Err(ApiError::model_not_found(model));
        }

        self.context
            .session_manager
            .set_session_model(session_id, model)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn create_session(&self) -> Result<String, ApiError> {
        let session_id = caelix_api::utils::generate_session_id();
        self.context
            .session_manager
            .create_session_config(session_id.clone())
            .await
            .map_err(|e| ApiError::InternalError(format!("创建会话配置失败: {}", e)))?;
        Ok(session_id)
    }

    async fn create_session_with_id(&self, session_id: String) -> Result<(), ApiError> {
        validate_session_id(&session_id)?;

        self.context
            .session_manager
            .create_session_config(session_id)
            .await
            .map_err(|e| ApiError::InternalError(format!("创建会话配置失败: {}", e)))?;
        Ok(())
    }

    async fn session_exists(&self, session_id: &str) -> Result<bool, ApiError> {
        validate_session_id(session_id)?;
        Ok(self
            .context
            .session_manager
            .session_exists(session_id)
            .await)
    }

    async fn list_agents(&self) -> Vec<String> {
        let agents = self.context.agent_manager.get_all().await;
        agents.iter().map(|a| a.get_spec().name.clone()).collect()
    }

    async fn set_variable(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.context.variable_manager.set_global(key, value).await;
        Ok(())
    }

    async fn get_variable(&self, key: &str) -> Result<Option<String>, ApiError> {
        Ok(self.context.variable_manager.get_global(key).await)
    }

    async fn delete_variable(&self, key: &str) -> Result<(), ApiError> {
        self.context.variable_manager.delete_global(key).await;
        Ok(())
    }

    async fn list_variables(&self) -> Result<HashMap<String, String>, ApiError> {
        Ok(self.context.variable_manager.list_globals().await)
    }

    async fn set_space_variable(
        &self,
        space: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ApiError> {
        self.context
            .variable_manager
            .set_space_var(space, key, value)
            .await;
        Ok(())
    }

    async fn get_space_variable(&self, space: &str, key: &str) -> Result<Option<String>, ApiError> {
        Ok(self
            .context
            .variable_manager
            .get_space_var(space, key)
            .await)
    }

    async fn delete_space_variable(&self, space: &str, key: &str) -> Result<(), ApiError> {
        self.context
            .variable_manager
            .delete_space_var(space, key)
            .await;
        Ok(())
    }

    async fn list_space_variables(&self, space: &str) -> Result<HashMap<String, String>, ApiError> {
        Ok(self.context.variable_manager.list_space_vars(space).await)
    }

    async fn replace_variables(&self, text: &str, space: Option<&str>) -> Result<String, ApiError> {
        let replacer = VariableReplacer::new(self.context.variable_manager.clone());
        Ok(replacer.replace_async(text, space).await)
    }

    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, ApiError> {
        validate_session_id(session_id)?;

        if !self
            .context
            .session_manager
            .session_exists(session_id)
            .await
        {
            return Err(ApiError::session_not_found(session_id));
        }

        self.context
            .session_manager
            .get_session_messages(session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError> {
        if let Some(sid) = session_id {
            validate_session_id(sid)?;
        }

        let task_manager = match &self.context.task_manager {
            Some(tm) => tm,
            None => {
                return Err(ApiError::InternalError(
                    "TaskManager not initialized".to_string(),
                ));
            }
        };

        Ok(task_manager.list_tasks(session_id).await)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError> {
        let session_ids = self.context.session_manager.list_sessions().await;
        let session_manager = self.context.session_manager.clone();

        // 使用 buffer_unordered 限制并发数，避免大量会话时同时压垮存储和内存
        let mut summaries: Vec<SessionSummary> = Vec::with_capacity(session_ids.len());
        let mut stream = stream::iter(session_ids.into_iter().map(|session_id| {
            let sm = session_manager.clone();
            async move {
                let config = sm.get_session_config(&session_id).await?;
                // 只取第一条消息用于摘要，避免全量加载
                let messages = sm.get_session_messages(&session_id).await.unwrap_or_default();
                let first_msgs: Vec<_> = messages.into_iter().take(1).collect();
                let summary = summarize_first_message(&first_msgs);
                Some(SessionSummary {
                    session_id,
                    created_at: config.created_at,
                    summary,
                })
            }
        }))
        .buffer_unordered(LIST_SESSIONS_MAX_CONCURRENCY);

        while let Some(summary) = stream.next().await {
            if let Some(s) = summary {
                summaries.push(s);
            }
        }

        Ok(summaries)
    }

    async fn get_providers(&self) -> Result<Vec<ProviderInfo>, ApiError> {
        // 先 clone 出 provider 列表，释放锁后再构造 ProviderInfo
        let providers: Vec<(String, Arc<dyn LlmProvider>)> = {
            let provider_manager = self.context.llm_provider_manager.read().await;
            provider_manager.get_all_providers()
        };

        let result = providers
            .into_iter()
            .map(|(name, provider)| {
                let config = provider.config();
                let llm_type = match config.llm_type {
                    caelix_api::provider::LlmType::OpenAI => "openai".to_string(),
                };
                let models: Vec<String> = config.models.values().cloned().collect();
                ProviderInfo {
                    name,
                    llm_type,
                    models,
                }
            })
            .collect();

        Ok(result)
    }

    async fn get_provider_models(&self, provider_name: &str) -> Result<Vec<String>, ApiError> {
        let provider_manager = self.context.llm_provider_manager.read().await;

        let provider = provider_manager
            .get_provider(provider_name)
            .ok_or_else(|| ApiError::provider_not_found(provider_name))?;

        let config = provider.config();
        let models: Vec<String> = config.models.values().cloned().collect();

        Ok(models)
    }

    async fn get_session_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<NotificationMessage>, ApiError> {
        validate_session_id(session_id)?;

        // 通知消息不再持久化，此接口始终返回错误
        // 请通过 subscribe_chat_stream 订阅实时通知
        Err(ApiError::InternalError(
            "通知消息不再持久化，请通过 subscribe_chat_stream 订阅实时通知".to_string(),
        ))
    }

    async fn chat_stream_async(&self, request: ChatRequest) -> Result<ChatAsyncResult, ApiError> {
        validate_session_id(&request.session_id)?;

        // 1. 如果会话不存在则创建
        if !self
            .context
            .session_manager
            .session_exists(&request.session_id)
            .await
        {
            self.context
                .session_manager
                .create_session_config(request.session_id.clone())
                .await
                .map_err(|e| ApiError::InternalError(e.to_string()))?;
        }

        // 2. 生成 request_id 和 span_id（trace_id 由 RuntimeContext 内部生成）
        let request_id = caelix_api::utils::generate_request_id();
        let span_id = caelix_api::utils::generate_span_id();

        // 3. 确定 provider 和 model（单次加锁）
        let (provider_name, model_name, provider) = {
            let pm = self.context.llm_provider_manager.read().await;
            let all_providers = pm.get_all_providers();
            if all_providers.is_empty() {
                return Err(ApiError::provider_not_found("（无已注册的 Provider）"));
            }

            let provider_name = request
                .provider
                .as_deref()
                .or_else(|| all_providers.first().map(|(n, _)| n.as_str()))
                .unwrap_or_default()
                .to_string();

            let provider = pm
                .get_provider(&provider_name)
                .cloned()
                .ok_or_else(|| ApiError::provider_not_found(&provider_name))?;

            let config = provider.config();
            let model_name = request
                .model
                .as_deref()
                .map(str::to_string)
                .or_else(|| config.default_model.clone())
                .or_else(|| config.models.values().next().cloned())
                .unwrap_or_default();

            (provider_name, model_name, provider)
        };

        // 4. 确定工作目录（用于项目级配置加载和变量替换的 space）
        let work_dir: PathBuf = request
            .work_dir
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| ApiError::InternalError("无法获取工作目录".to_string()))?;

        if work_dir.as_os_str().is_empty() {
            return Err(ApiError::InternalError("工作目录不能为空".to_string()));
        }

        // 5. 确保项目配置已加载（基于 work_dir，懒加载）
        let context_clone = self.context.clone();
        let overlay = context_clone.config_overlay();
        if let Err(e) = overlay.ensure_project_config_loaded(&work_dir).await {
            tracing::warn!(error = %e, "Failed to load project config");
        }

        // 6. 获取 agent_spec（通过 ConfigOverlay 优先获取项目级配置）
        let agent_name = request.agent.as_deref().unwrap_or("default");
        let agent_spec = overlay
            .get_agent_spec_for_work_dir(&work_dir, agent_name)
            .await
            .ok_or_else(|| ApiError::agent_not_found(agent_name))?;

        // 7. 构建 LlmConfig
        let config = LlmConfig {
            model_name: model_name.clone(),
        };

        // 8. 获取历史消息
        let history_messages = context_clone
            .session_manager
            .get_session_messages(&request.session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;

        // 9. 在后台启动任务，绑定 RuntimeContext
        // 顺序很重要：先 register cancel_token 占位，再 spawn，spawn 后回填 handle。
        // 这样消除 spawn→register 的竞态窗口——任何在 spawn 之前调用的 stop_agent
        // 都能通过 cancel_token 通知任务（任务 spawn 后首个检查点即退出）。
        let debug_enabled = context_clone.env_config.debug_enabled();
        let agent_run_manager = context_clone.agent_run_manager.clone();
        let cancel_token = caelix_api::cancel::CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let request_session_id = request.session_id.clone();
        let request_id_clone = request_id.clone();
        let run_id = agent_run_manager.register(request_session_id.clone(), cancel_token);

        let join_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            // RunGuard 确保任务退出时（正常、panic、abort）从 AgentRunManager 注销
            let _guard = caelix_runtime::agent_run_manager::RunGuard::new(
                agent_run_manager.clone(),
                request_session_id.clone(),
                run_id,
            );

            // 创建 RuntimeContext 并绑定到 task_local 作用域
            let runtime_ctx = Arc::new(RuntimeContext::new(
                Some(request_session_id.clone()),
                Some(request_id_clone.clone()),
                work_dir,
                provider_name,
                model_name,
                debug_enabled,
                cancel_token_clone,
            ));

            let ctx_for_scope = runtime_ctx.clone();

            let fut = async move {
                let mut messages: Vec<ChatMessage> = Vec::new();
                for msg in history_messages.iter() {
                    if msg.r#type != AgentMessageType::Msg {
                        continue;
                    }
                    match serde_json::from_str::<ChatMessage>(&msg.content) {
                        Ok(chat_msg) => messages.push(chat_msg),
                        Err(e) => {
                            tracing::warn!(
                                session_id = %request_session_id,
                                error = %e,
                                content_len = msg.content.len(),
                                "Failed to deserialize history message, skipping"
                            );
                        }
                    }
                }

                // 如果带用户消息则添加
                // 注意：message=None 的 "resume" 路径尚未实现，当前仅跳过用户消息
                if let Some(original_message) = request.message.clone() {
                    // 使用 RuntimeContext 中的 work_dir 作为变量替换的 space
                    let space = runtime_ctx
                        .get_work_dir()
                        .to_str()
                        .map(|s| s.to_string());
                    let replacer = VariableReplacer::new(context_clone.variable_manager.clone());
                    let user_message = replacer
                        .replace_async(&original_message, space.as_deref())
                        .await;

                    messages.push(ChatMessage::user(user_message.clone()));

                    // 发送用户消息到消息总线（只有带用户消息才发）
                    let user_msg = AgentMessage {
                        session_id: request_session_id.clone(),
                        request_id: request_id_clone.clone(),
                        span_id: runtime_ctx.get_span_id().to_string(),
                        trace_id: runtime_ctx.get_trace_id().to_string(),
                        r#type: AgentMessageType::Msg,
                        timestamp: chrono::Utc::now(),
                        content: user_message,
                        agent_name: request.agent.clone(),
                        usage: None,
                    };
                    if context_clone.message_bus.send_agent(user_msg).is_err() {
                        tracing::warn!("Failed to send user message to message bus");
                    }
                }

                // 使用 caelix_agent::run_agent（内部通过 RuntimeContext + ContextProvider 获取 message_bus）
                let _ = run_agent(agent_spec, messages, provider, &config)
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            session_id = %request_session_id,
                            error = %e,
                            "Agent execution failed"
                        );
                    });
            };

            let _ = fut.with_runtime_ctx(ctx_for_scope).await;
            // _guard 在此 drop，自动调用 unregister(session_id, run_id)
        });

        // 回填 join_handle，使 stop_agent 可以等待/强制中止
        self.context
            .agent_run_manager
            .set_handles(&request.session_id, run_id, join_handle);

        // 10. 立即返回完整信息
        Ok(ChatAsyncResult {
            request_id,
            span_id,
            session_id: request.session_id,
        })
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), ApiError> {
        validate_session_id(session_id)?;

        // 1. 通过 SessionManager 查找并更新审批状态（一步完成查找 + 写回存储）
        let updated_agent_msg = self
            .context
            .session_manager
            .update_tool_approval(session_id, tool_call_id, approved)
            .await
            .map_err(|e| ApiError::InternalError(format!("更新审批状态失败: {}", e)))?
            .ok_or_else(|| {
                ApiError::InternalError(format!(
                    "未在 session {} 中找到 tool_call_id = {} 的 Assistant 消息",
                    session_id, tool_call_id
                ))
            })?;

        // 2. 从返回的 (index, AgentMessage) 中提取 agent_name 和反序列化 ChatMessage
        let (_, updated_msg) = updated_agent_msg;
        let agent_name = updated_msg
            .agent_name
            .as_deref()
            .unwrap_or("default");
        let chat_msg: ChatMessage = serde_json::from_str(&updated_msg.content)
            .map_err(|e| ApiError::InternalError(format!("反序列化 ChatMessage 失败: {}", e)))?;

        // 找到 tool_call 对应的工具名
        let tool_name = chat_msg
            .tool_calls
            .as_ref()
            .and_then(|tcs| tcs.iter().find(|tc| tc.id == tool_call_id))
            .map(|tc| tc.name.clone())
            .unwrap_or_default();

        // 3. 根据审批结果执行后续逻辑
        if approved {
            self.execute_approved_tool(
                session_id,
                tool_call_id,
                agent_name,
                &chat_msg,
                &tool_name,
            )
            .await?;
        } else {
            self.append_rejection_result(session_id, tool_call_id, &tool_name)
                .await?;
        }

        Ok(())
    }

    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError> {
        validate_session_id(session_id)?;

        if !self
            .context
            .session_manager
            .session_exists(session_id)
            .await
        {
            return Err(ApiError::session_not_found(session_id));
        }

        let (history, stream) = self
            .context
            .session_manager
            .subscribe_agent(session_id.to_string())
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;

        let history_stream = stream::iter(history);

        // 合并历史流和实时流
        // 错误处理：将接收错误转为 Event 消息后终止流，避免伪装为正常 Chunk
        let session_id_owned = session_id.to_string();
        let live_stream = stream.scan(false, move |errored, r| {
            if *errored {
                return future::ready(None);
            }
            match r {
                Ok(msg) => future::ready(Some(msg)),
                Err(e) => {
                    *errored = true;
                    future::ready(Some(make_agent_message(
                        &session_id_owned,
                        AgentMessageType::Event,
                        format!("订阅错误: {:?}", e),
                        None,
                    )))
                }
            }
        });

        let merged_stream = history_stream.chain(live_stream);

        Ok(Box::pin(merged_stream))
    }

    async fn get_session_usage(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionUsageView>, ApiError> {
        validate_session_id(session_id)?;

        let tracker = self
            .context
            .usage_tracker()
            .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;

        // 按 session 实际配置的 provider 获取 ctx_window_tokens
        let session_provider = self
            .context
            .session_manager
            .get_session_config(session_id)
            .await
            .and_then(|cfg| cfg.provider);

        let ctx_window_tokens = {
            let provider_manager = self.context.llm_provider_manager.read().await;
            if let Some(ref prov_name) = session_provider {
                provider_manager
                    .get_provider(prov_name)
                    .and_then(|p| p.config().ctx_window_tokens)
            } else {
                provider_manager
                    .get_all_providers()
                    .first()
                    .and_then(|(_, p)| p.config().ctx_window_tokens)
            }
        };

        Ok(tracker
            .snapshot_session(session_id, ctx_window_tokens)
            .await)
    }

    async fn get_global_usage(&self) -> Result<GlobalUsageView, ApiError> {
        let tracker = self
            .context
            .usage_tracker()
            .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;
        Ok(tracker.snapshot_global().await)
    }

    async fn stop_agent(&self, session_id: &str) -> Result<bool, ApiError> {
        validate_session_id(session_id)?;

        Ok(self.context.agent_run_manager.stop_agent(session_id).await)
    }
}
