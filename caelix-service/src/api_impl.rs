use crate::api_trait::CaelixApi;
use crate::types::{ChatAsyncResult, ChatRequest, ProviderInfo, SessionSummary};
use async_trait::async_trait;
use caelix_api::agent::AgentOutputChunk;
use caelix_api::context::{ContextFutureExt, ContextProvider};
use caelix_api::error::ApiError;
use caelix_api::message::{AgentMessage, AgentMessageType, NotificationMessage};
use caelix_api::provider::{ChatMessage, LlmConfig, SessionUsageView, GlobalUsageView};
use caelix_api::task::TaskMeta;
use caelix_runtime::context::CaelixContext;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::pin::Pin;
use std::sync::Arc;

/// 执行 Agent 并发送结果到消息总线
///
/// 消息总线的获取、各类分片 → 消息类型的映射全部下沉到
/// `caelix_agent::run_agent` 内部处理（通过 `RuntimeContext` +
/// `ContextProvider` 从 API 包中获取）。
/// 这里只负责将执行结果包装为 anyhow::Error 并返回。
async fn execute_agent_with_messaging(
    agent_spec: Arc<caelix_api::agent::AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
) -> Result<String, anyhow::Error> {
    caelix_agent::run_agent(agent_spec, messages, provider, config)
        .await
        .map_err(|e| anyhow::anyhow!("Agent execution error: {:?}", e))
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
    #[allow(dead_code)] // 为将来外部访问预留
    pub fn message_bus(&self) -> &Arc<caelix_message::MessageBus> {
        &self.context.message_bus
    }

    /// 获取 SessionManager 引用
    pub fn session_manager(&self) -> &caelix_message::SessionManager {
        &self.context.session_manager
    }
}

#[async_trait]
impl CaelixApi for CaelixApiImpl {
    fn get_default_provider(&self) -> String {
        "暂无".to_string()
    }

    fn get_default_model(&self) -> String {
        // 从 context 中读取初始化时设置的默认 model
        "暂无".to_string()
    }

    async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<(), ApiError> {
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
        // 这里可以添加模型验证逻辑
        self.context
            .session_manager
            .set_session_model(session_id, model)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn create_session(&self) -> String {
        // 使用中央ID生成器生成 session_id
        let session_id = caelix_api::utils::generate_session_id();
        // 在 runtime SessionManager 中创建配置（等待完成）
        if let Err(e) = self
            .context
            .session_manager
            .create_session_config(session_id.clone())
            .await
        {
            eprintln!("⚠️  创建会话配置失败: {:?}", e);
        }
        session_id
    }

    async fn create_session_with_id(&self, session_id: String) {
        // 使用指定的 session_id 创建会话配置
        // 安全校验：拒绝路径穿越字符，防止构造 session/../../.. 的路径
        if session_id.contains('/') || session_id.contains('\\') || session_id.contains("..") {
            eprintln!("⚠️  创建会话配置失败: session_id 包含非法字符");
            return;
        }
        if let Err(e) = self
            .context
            .session_manager
            .create_session_config(session_id.clone())
            .await
        {
            eprintln!("⚠️  创建会话配置失败: {:?}", e);
        }
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        self.context
            .session_manager
            .session_exists(session_id)
            .await
    }

    async fn list_agents(&self) -> Vec<String> {
        let agents = self.context.agent_manager.get_all().await;
        agents.iter().map(|a| a.get_spec().name.clone()).collect()
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<AgentOutputChunk, ApiError>>, ApiError> {
        todo!()
    }

    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, ApiError> {
        // 验证会话存在
        if !self
            .context
            .session_manager
            .session_exists(session_id)
            .await
        {
            return Err(ApiError::session_not_found(session_id));
        }

        // 从 SessionManager 获取消息
        self.context
            .session_manager
            .get_session_messages(session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError> {
        // 检查是否有 task_manager
        let task_manager = match &self.context.task_manager {
            Some(tm) => tm,
            None => {
                return Err(ApiError::InternalError(
                    "TaskManager not initialized".to_string(),
                ));
            }
        };

        // 获取任务列表（TaskMeta 现在已经是 caelix_api::task::TaskMeta）
        let task_metas = task_manager.list_tasks(session_id).await;

        Ok(task_metas)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError> {
        // 获取所有会话ID
        let session_ids = self.context.session_manager.list_sessions().await;

        let mut summaries = Vec::new();
        for session_id in session_ids {
            // 获取会话配置
            if let Some(config) = self
                .context
                .session_manager
                .get_session_config(&session_id)
                .await
            {
                // 获取首条消息作为摘要
                let messages = self
                    .context
                    .session_manager
                    .get_session_messages(&session_id)
                    .await
                    .unwrap_or_default();

                let summary = messages
                    .first()
                    .map(|msg| {
                        // AgentMessage.content 现在是 ChatMessage 的 JSON 字符串
                        let actual_content = if let Ok(chat_msg) =
                            serde_json::from_str::<ChatMessage>(&msg.content)
                        {
                            chat_msg.content
                        } else {
                            msg.content.clone()
                        };

                        let chars: Vec<char> = actual_content.chars().collect();
                        if chars.len() > 15 {
                            chars[..15].iter().collect::<String>() + "..."
                        } else {
                            actual_content
                        }
                    })
                    .unwrap_or_else(|| "新会话".to_string());

                summaries.push(SessionSummary {
                    session_id,
                    created_at: config.created_at,
                    summary,
                });
            }
        }

        Ok(summaries)
    }

    async fn get_providers(&self) -> Result<Vec<ProviderInfo>, ApiError> {
        let provider_manager = self.context.llm_provider_manager.read().await;

        let providers = provider_manager
            .get_all_providers()
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

        Ok(providers)
    }

    async fn get_provider_models(&self, provider_name: &str) -> Result<Vec<String>, ApiError> {
        let provider_manager = self.context.llm_provider_manager.read().await;

        let provider = provider_manager
            .get_provider(provider_name)
            .ok_or_else(|| ApiError::provider_not_found(provider_name))?;

        // 从 provider config 中获取 models
        let config = provider.config();
        let models: Vec<String> = config.models.values().cloned().collect();

        Ok(models)
    }

    async fn get_session_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<NotificationMessage>, ApiError> {
        // 验证会话存在
        if !self
            .context
            .session_manager
            .session_exists(session_id)
            .await
        {
            return Err(ApiError::session_not_found(session_id));
        }

        // 注意：通知消息不再持久化，只能从内存缓冲中获取
        // 这里返回空列表，实际应该通过订阅接口获取
        Ok(Vec::new())
    }

    async fn chat_stream_async(&self, request: ChatRequest) -> Result<ChatAsyncResult, ApiError> {
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

        // 2. 生成 request_id、span_id（为将来扩展预留，trace_id 由 RuntimeContext 内部生成）
        let request_id = caelix_api::utils::generate_request_id();
        let span_id = caelix_api::utils::generate_span_id();
        let _trace_id = caelix_api::utils::generate_trace_id();

        // 3. 确定 provider 和 model
        let default_provider = self.get_default_provider();
        let default_model = self.get_default_model();
        let provider_name = request.provider.clone().unwrap_or(default_provider);
        let model_name = request.model.clone().unwrap_or(default_model);

        // 4. 克隆必要的依赖
        let ctx_clone = self.context.clone();
        let request_clone = request.clone();
        let request_id_clone = request_id.clone();

        // 5.2 在 RuntimeContext scope 中执行
        // 获取 agent (Arc<dyn Agent>)
        let agent_name = request_clone.agent.as_deref().unwrap_or("default");
        let agent = ctx_clone
            .agent_manager
            .get(agent_name)
            .await
            .ok_or_else(|| ApiError::agent_not_found(agent_name))?;

        // 获取 agent_spec 用于日志和消息传递
        let agent_spec = agent.get_spec();

        // 获取 provider
        let provider = {
            let provider_manager = ctx_clone.llm_provider_manager.read().await;
            provider_manager
                .get_provider(&provider_name)
                .ok_or_else(|| ApiError::provider_not_found(&provider_name))?
                .clone()
        };

        // 构建 LlmConfig
        let config = LlmConfig {
            model_name: model_name.to_string(),
        };

        // 构建消息列表
        let history_messages = ctx_clone
            .session_manager
            .get_session_messages(&request_clone.session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;

        // 获取工作目录（用于 RuntimeContext）
        let work_dir = ctx_clone
            .env_config()
            .caelix_home()
            .join("sessions")
            .join(request_clone.session_id.clone());

        // 5. 在后台启动任务，绑定 RuntimeContext
        // 顺序很重要：先 register cancel_token 占位，再 spawn，spawn 后回填 handle。
        // 这样消除 spawn→register 的竞态窗口——任何在 spawn 之前调用的 stop_agent
        // 都能通过 cancel_token 通知任务（任务 spawn 后首个检查点即退出）。
        let debug_enabled = ctx_clone.env_config().debug_enabled();
        let agent_run_manager = ctx_clone.agent_run_manager.clone();
        let arm_for_spawn = agent_run_manager.clone();
        let cancel_token = caelix_api::cancel::CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let run_id = agent_run_manager.register(
            request_clone.session_id.clone(),
            cancel_token,
        );

        let join_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            // RunGuard 确保任务退出时（正常、panic、abort）从 AgentRunManager 注销
            let _guard = caelix_runtime::agent_run_manager::RunGuard::new(
                arm_for_spawn.clone(),
                request_clone.session_id.clone(),
                run_id,
            );

            // 创建 RuntimeContext 并绑定到 task_local 作用域
            // 所有子异步（agent、message_bus_hook 等）都能通过 try_current() 访问
            let runtime_ctx = Arc::new(caelix_api::context::RuntimeContext::new(
                Some(request_clone.session_id.clone()),
                Some(request_id_clone.clone()),
                work_dir,
                provider_name.clone(),
                model_name.clone(),
                debug_enabled,
                cancel_token_clone,
            ));

            // 先克隆一份给 scope 使用，再把 runtime_ctx move 进 async block
            let ctx_for_scope = runtime_ctx.clone();

            let fut = async move {
                let mut messages: Vec<ChatMessage> = Vec::new();
                for msg in history_messages.iter() {
                    if msg.r#type == AgentMessageType::Msg {
                        match serde_json::from_str::<ChatMessage>(&msg.content) {
                            Ok(chat_msg) => messages.push(chat_msg),
                            Err(_) => {
                                messages.push(ChatMessage {
                                    role: "user".to_string(),
                                    content: msg.content.clone(),
                                    tool_calls: None,
                                    tool_call_id: None,
                                });
                            }
                        }
                    }
                }

                // 如果带用户消息则添加
                if let Some(user_message) = request_clone.message.clone() {
                    messages.push(ChatMessage::user(user_message.clone()));

                    // 发送用户消息到消息总线（只有带用户消息才发）
                    let user_msg = AgentMessage {
                        session_id: request_clone.session_id.clone(),
                        request_id: request_id_clone.clone(),
                        span_id: runtime_ctx.get_span_id().to_string(),
                        trace_id: runtime_ctx.get_trace_id().to_string(),
                        r#type: AgentMessageType::Msg,
                        timestamp: chrono::Utc::now(),
                        content: user_message,
                        agent_name: request_clone.agent.clone(),
                        usage: None,
                    };
                    let _ = ctx_clone.message_bus.send_agent(user_msg);
                }

                // 使用 caelix_agent::run_agent（内部通过 RuntimeContext + ContextProvider 获取 message_bus）
                let _ = execute_agent_with_messaging(agent_spec, messages, provider, &config).await
                    .inspect_err(|e| {
                        tracing::error!(
                            session_id = %request_clone.session_id,
                            error = %e,
                            "Agent execution failed"
                        );
                    });
            };

            let _ = fut.with_runtime_ctx(ctx_for_scope).await;
            // _guard 在此 drop，自动调用 unregister(session_id, run_id)
        });

        // 回填 join_handle 与 abort_handle，使 stop_agent 可以等待/强制中止
        agent_run_manager.set_handles(&request.session_id, run_id, join_handle);

        // 6. 立即返回完整信息
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
        // 安全校验：拒绝路径穿越字符，防止构造 session/../../.. 的路径
        if session_id.contains('/') || session_id.contains('\\') || session_id.contains("..") {
            return Err(ApiError::InternalError("session_id 包含非法字符".into()));
        }

        // 1. 先从历史中找到 ChatMessage 以及 tool_call 的位置
        let history_messages = self
            .context
            .session_manager
            .get_session_messages(session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;

        let mut found_chat_msg: Option<(usize, ChatMessage, String, String)> = None;
        for (msg_idx, msg) in history_messages.iter().enumerate().rev() {
            if msg.r#type != AgentMessageType::Msg {
                continue;
            }
            if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&msg.content) {
                if chat_msg.role != "assistant" {
                    continue;
                }
                if let Some(tool_calls) = &chat_msg.tool_calls
                    && tool_calls.iter().any(|t| t.id == tool_call_id)
                {
                    // 记录相关工具名与参数
                    let (tool_name, args) = tool_calls
                        .iter()
                        .find(|t| t.id == tool_call_id)
                        .map(|t| (t.name.clone(), t.arguments.clone()))
                        .unwrap_or_else(|| (String::new(), serde_json::Value::Null));
                    found_chat_msg = Some((msg_idx, chat_msg, tool_name, args.to_string()));
                    break;
                }
            }
        }

        let (msg_idx, chat_msg, tool_name, _args) = match found_chat_msg {
            Some(v) => v,
            None => {
                return Err(ApiError::InternalError(format!(
                    "未在 session {} 中找到 tool_call_id = {} 的 Assistant 消息",
                    session_id, tool_call_id
                )));
            }
        };

        // 2. 更新 approval_state 并写回存储
        let mut updated_chat_msg = chat_msg.clone();
        if let Some(tool_calls) = updated_chat_msg.tool_calls.as_mut() {
            for tc in tool_calls.iter_mut() {
                if tc.id == tool_call_id {
                    tc.approval_state = if approved {
                        Some(caelix_api::tool::ToolCallApprovalState::Approved)
                    } else {
                        Some(caelix_api::tool::ToolCallApprovalState::Rejected)
                    };
                    break;
                }
            }
        }

        // 构造新 AgentMessage，写入存储（replace_agent_message 在 FileStorage 中实现）
        let new_content = serde_json::to_string(&updated_chat_msg).map_err(|e| {
            ApiError::InternalError(format!("序列化更新后的 ChatMessage 失败: {}", e))
        })?;

        let session_for_storage = session_id.to_string();
        let storage = self.context.session_manager.get_storage();
        let new_agent_msg = AgentMessage {
            session_id: session_for_storage.clone(),
            request_id: String::new(),
            span_id: String::new(),
            trace_id: String::new(),
            r#type: AgentMessageType::Msg,
            timestamp: chrono::Utc::now(),
            content: new_content,
            agent_name: None,
            usage: None,
        };
        storage
            .replace_agent_message(&session_for_storage, msg_idx, &new_agent_msg)
            .await
            .map_err(|e| ApiError::InternalError(format!("写入审批状态失败: {}", e)))?;

        // 3. 若批准：执行该工具一次，追加 tool_result 消息
        if approved {
            // 通过 agent_manager 获取 agent_spec
            let agent_name = "default";
            let agent = self
                .context
                .agent_manager
                .get(agent_name)
                .await
                .ok_or_else(|| ApiError::agent_not_found(agent_name))?;
            let agent_spec = agent.get_spec();

            // 找到对应 tool_call 并执行
            let mut tool_result_text = String::new();
            if let Some(tcs) = &updated_chat_msg.tool_calls {
                for tc in tcs.iter() {
                    if tc.id == tool_call_id {
                        // 解析参数
                        let args_json = match &tc.arguments {
                            serde_json::Value::String(s) => {
                                serde_json::from_str::<serde_json::Value>(s)
                                    .unwrap_or(serde_json::Value::String(s.clone()))
                            }
                            other => other.clone(),
                        };

                        // 查找工具
                        let tool = agent_spec.tools.iter().find(|t| t.name() == tc.name);
                        let tool = match tool {
                            Some(t) => t,
                            None => {
                                tool_result_text = format!("[ERROR] Tool '{}' not found", tc.name);
                                break;
                            }
                        };

                        let result = tool.execute(args_json).await;
                        tool_result_text = match result.error {
                            Some(e) => format!("[ERROR] {}", e),
                            None => result.output,
                        };
                        break;
                    }
                }
            }

            // 构造 ChatMessage::tool 并发送到消息总线 + 持久化
            let chat_tool_msg = ChatMessage {
                role: "tool".to_string(),
                content: tool_result_text.clone(),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.to_string()),
            };
            // 通过 storage 写入
            let agent_msg_for_storage = AgentMessage {
                session_id: session_for_storage.clone(),
                request_id: String::new(),
                span_id: String::new(),
                trace_id: String::new(),
                r#type: AgentMessageType::Msg,
                timestamp: chrono::Utc::now(),
                content: serde_json::to_string(&chat_tool_msg).unwrap_or_else(|_| {
                    format!("{{\"role\":\"tool\",\"content\":\"{}\"}}", tool_result_text)
                }),
                agent_name: Some(tool_name.clone()),
                usage: None,
            };
            let _ = storage
                .append_agent_message(&agent_msg_for_storage)
                .await
                .map_err(|e| ApiError::InternalError(format!("持久化 tool_result 失败: {}", e)))?;

            // 发送一条 Event 消息让前端感知（通过消息总线）
            let event_msg = AgentMessage {
                session_id: session_for_storage.clone(),
                request_id: String::new(),
                span_id: String::new(),
                trace_id: String::new(),
                r#type: AgentMessageType::Event,
                timestamp: chrono::Utc::now(),
                content: format!(
                    "[已批准] tool_call_id={}, tool_name={}",
                    tool_call_id, tool_name
                ),
                agent_name: Some(tool_name),
                usage: None,
            };
            let _ = self.context.message_bus.send_agent(event_msg);
        } else {
            // 拒绝：追加一条拒绝文本 tool_result 消息
            let chat_tool_msg = ChatMessage {
                role: "tool".to_string(),
                content: format!("[REJECTED] tool_call_id={} 已被拒绝执行", tool_call_id),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.to_string()),
            };
            let agent_msg_for_storage = AgentMessage {
                session_id: session_for_storage.clone(),
                request_id: String::new(),
                span_id: String::new(),
                trace_id: String::new(),
                r#type: AgentMessageType::Msg,
                timestamp: chrono::Utc::now(),
                content: serde_json::to_string(&chat_tool_msg).unwrap_or_default(),
                agent_name: None,
                usage: None,
            };
            let _ = storage.append_agent_message(&agent_msg_for_storage).await;

            // 发送一条 Event 消息让前端感知
            let event_msg = AgentMessage {
                session_id: session_for_storage.clone(),
                request_id: String::new(),
                span_id: String::new(),
                trace_id: String::new(),
                r#type: AgentMessageType::Event,
                timestamp: chrono::Utc::now(),
                content: format!(
                    "[已拒绝] tool_call_id={}, tool_name={}",
                    tool_call_id,
                    chat_msg
                        .tool_calls
                        .as_ref()
                        .map(|t| t
                            .iter()
                            .find(|tc| tc.id == tool_call_id)
                            .map(|tc| tc.name.clone())
                            .unwrap_or_default())
                        .unwrap_or_default()
                ),
                agent_name: None,
                usage: None,
            };
            let _ = self.context.message_bus.send_agent(event_msg);
        }

        Ok(())
    }

    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError> {
        // 验证会话存在
        if !self
            .context
            .session_manager
            .session_exists(session_id)
            .await
        {
            return Err(ApiError::session_not_found(session_id));
        }

        // 使用 SessionManager 的 subscribe_agent 方法
        let (history, stream) = self
            .context
            .session_manager
            .subscribe_agent(session_id.to_string())
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;

        // 先发送历史消息，然后订阅实时消息
        // 将 Vec<AgentMessage> 转换为 Stream
        use futures::stream;

        let history_stream = stream::iter(history);

        // 合并历史流和实时流
        let session_id_owned = session_id.to_string();
        let merged_stream = history_stream.chain(stream.map(move |result| {
            result.unwrap_or_else(|e| {
                // 处理接收错误，创建一个错误消息
                AgentMessage {
                    session_id: session_id_owned.clone(),
                    request_id: String::new(),
                    span_id: String::new(),
                    trace_id: String::new(),
                    r#type: AgentMessageType::Chunk,
                    timestamp: chrono::Utc::now(),
                    content: format!("订阅错误: {:?}", e),
                    agent_name: None,
                    usage: None,
                }
            })
        }));

        Ok(Box::pin(merged_stream))
    }

    async fn get_session_usage(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionUsageView>, ApiError> {
        let tracker = self
            .context
            .usage_tracker()
            .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;
        let ctx_window_tokens = self
            .context
            .llm_provider_manager
            .read()
            .await
            .get_all_providers()
            .first()
            .cloned()
            .and_then(|(_name, p)| p.config().ctx_window_tokens);
        Ok(tracker.snapshot_session(session_id, ctx_window_tokens).await)
    }

    async fn get_global_usage(&self) -> Result<GlobalUsageView, ApiError> {
        let tracker = self
            .context
            .usage_tracker()
            .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;
        Ok(tracker.snapshot_global().await)
    }

    async fn stop_agent(&self, session_id: &str) -> Result<bool, ApiError> {
        // agent_run_manager 在 CaelixContext 中始终初始化；None 分支为 trait 通用性保留
        let arm = self
            .context
            .agent_run_manager()
            .ok_or_else(|| ApiError::InternalError("AgentRunManager 未初始化".to_string()))?;
        Ok(arm.stop_agent(session_id).await)
    }
}
