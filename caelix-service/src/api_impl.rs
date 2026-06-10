use std::sync::Arc;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::Stream;
use futures::StreamExt;
use std::pin::Pin;
use caelix_api::error::ApiError;
use caelix_api::agent::AgentOutputChunk;
use caelix_api::provider::{ChatMessage, LlmConfig};
use caelix_api::message::{AgentMessage, AgentMessageType, NotificationMessage};
use caelix_api::task::TaskMeta;
use caelix_runtime::context::CaelixContext;
use crate::api_trait::CaelixApi;
use crate::types::{ChatRequest, SessionSummary, ProviderInfo, ChatAsyncResult};

/// 临时执行器：执行 agent 并将流发送到消息总线
/// TODO: 后续会通过别的方式实现
#[allow(clippy::too_many_arguments)]
async fn execute_agent_with_messaging(
    agent_spec: Arc<caelix_api::agent::AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
    session_id: String,
    request_id: String,
    span_id: String,
    trace_id: String,
    agent_name: Option<String>,
    message_bus: Arc<caelix_message::MessageBus>,
) -> Result<String, anyhow::Error> {
    // 使用 loop_runner 执行 agent
    let stream = caelix_agent::run_agent(
        agent_spec,
        messages,
        provider,
        config,
    ).await;
    
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
                    trace_id: trace_id.clone(),
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
                        trace_id: trace_id.clone(),
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

/// API 核心实现
pub struct CaelixApiImpl {
    context: Arc<CaelixContext>,
}

impl CaelixApiImpl {
    pub fn new(context: Arc<CaelixContext>) -> Self {
        Self {
            context,
        }
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
        
        self.context.session_manager
            .set_session_provider(session_id, provider)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn set_session_model(&self, session_id: &str, model: &str) -> Result<(), ApiError> {
        // 这里可以添加模型验证逻辑
        self.context.session_manager
            .set_session_model(session_id, model)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn create_session(&self) -> String {
        // 使用中央ID生成器生成 session_id
        let session_id = caelix_api::utils::generate_session_id();
        // 在 runtime SessionManager 中创建配置（等待完成）
        if let Err(e) = self.context.session_manager
            .create_session_config(session_id.clone())
            .await
        {
            eprintln!("⚠️  创建会话配置失败: {:?}", e);
        }
        session_id
    }

    async fn create_session_with_id(&self, session_id: String) {
        // 使用指定的 session_id 创建会话配置
        if let Err(e) = self.context.session_manager
            .create_session_config(session_id.clone())
            .await
        {
            eprintln!("⚠️  创建会话配置失败: {:?}", e);
        }
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        self.context.session_manager.session_exists(session_id).await
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
        if !self.context.session_manager.session_exists(session_id).await {
            return Err(ApiError::session_not_found(session_id));
        }
        
        // 从 SessionManager 获取消息
        self.context.session_manager
            .get_session_messages(session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }

    async fn list_tasks(&self, session_id: Option<&str>) -> Result<Vec<TaskMeta>, ApiError> {
        // 检查是否有 task_manager
        let task_manager = match &self.context.task_manager {
            Some(tm) => tm,
            None => {
                return Err(ApiError::InternalError("TaskManager not initialized".to_string()));
            }
        };
        
        // 获取任务列表
        let task_metas = task_manager.list_tasks(session_id).await;
        
        // 转换为 caelix_api::task::TaskMeta
        let api_task_metas = task_metas.into_iter().map(|tm| TaskMeta {
            id: tm.task_id,
            kind: tm.kind,
            status: tm.status,
            created_at: tm.created_at,
            updated_at: tm.updated_at,
            payload: tm.task_payload,
        }).collect();
        
        Ok(api_task_metas)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, ApiError> {
        // 获取所有会话ID
        let session_ids = self.context.session_manager.list_sessions().await;
        
        let mut summaries = Vec::new();
        for session_id in session_ids {
            // 获取会话配置
            if let Some(config) = self.context.session_manager.get_session_config(&session_id).await {
                // 获取首条消息作为摘要
                let messages = self.context.session_manager
                    .get_session_messages(&session_id)
                    .await
                    .unwrap_or_default();
                
                let summary = messages.first()
                    .map(|msg| {
                        // AgentMessage.content 现在是 ChatMessage 的 JSON 字符串
                        let actual_content = if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&msg.content) {
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
        
        let providers = provider_manager.get_all_providers()
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

    async fn get_session_notifications(&self, session_id: &str) -> Result<Vec<NotificationMessage>, ApiError> {
        // 验证会话存在
        if !self.context.session_manager.session_exists(session_id).await {
            return Err(ApiError::session_not_found(session_id));
        }
        
        // 注意：通知消息不再持久化，只能从内存缓冲中获取
        // 这里返回空列表，实际应该通过订阅接口获取
        Ok(Vec::new())
    }

    async fn chat_stream_async(
        &self,
        request: ChatRequest,
    ) -> Result<ChatAsyncResult, ApiError> {
        // 1. 如果会话不存在则创建
        if !self.context.session_manager.session_exists(&request.session_id).await {
            self.context.session_manager
                .create_session_config(request.session_id.clone())
                .await
                .map_err(|e| ApiError::InternalError(e.to_string()))?;
        }
        
        // 2. 生成 request_id、span_id 和 trace_id（为将来扩展预留）
        let request_id = caelix_api::utils::generate_request_id();
        let span_id = caelix_api::utils::generate_span_id();
        let trace_id = caelix_api::utils::generate_trace_id();
        
        // 3. 确定 provider 和 model
        let default_provider = self.get_default_provider();
        let default_model = self.get_default_model();
        let provider_name = request.provider.clone()
            .unwrap_or(default_provider);
        let model_name = request.model.clone()
            .unwrap_or(default_model);
        
        // 4. 克隆必要的依赖
        let ctx_clone = self.context.clone();
        let request_clone = request.clone();
        let request_id_clone = request_id.clone();
        let span_id_clone = span_id.clone();
        let trace_id_clone = trace_id.clone();
        
        // 5.2 在 RuntimeContext scope 中执行
        // 获取 agent (Arc<dyn Agent>)
        let agent_name = request_clone.agent.as_deref().unwrap_or("default");
        let agent = ctx_clone.agent_manager.get(agent_name).await
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
        let history_messages = ctx_clone.session_manager
            .get_session_messages(&request_clone.session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;
        // 5. 在后台启动任务
        tokio::spawn(async move {
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
            
            // 添加当前用户消息
            messages.push(ChatMessage::user(request_clone.message.clone()));
            
            // 发送用户消息到消息总线
            let user_msg = AgentMessage {
                session_id: request_clone.session_id.clone(),
                request_id: request_id_clone.clone(),
                span_id: span_id_clone.clone(),  // 使用预先生成的 span_id
                trace_id: trace_id_clone.clone(),
                r#type: AgentMessageType::Msg,
                timestamp: chrono::Utc::now(),
                content: request_clone.message.clone(),
                agent_name: request_clone.agent.clone(),
            };
            let _ = ctx_clone.message_bus.send_agent(user_msg);
            
            // ✅ 使用本地执行器（会自动发送流到消息总线）
            let _result = execute_agent_with_messaging(
                agent_spec,
                messages,
                provider,
                &config,
                request_clone.session_id.clone(),
                request_id_clone.clone(),
                span_id_clone.clone(),
                trace_id_clone.clone(),
                request_clone.agent.clone(),
                ctx_clone.message_bus.clone(),
            ).await.map_err(|e| ApiError::InternalError(e.to_string()));
            
            let result = Ok::<_, ApiError>(());
            
            if let Err(e) = result {
                eprintln!("❌ chat_stream_async 执行失败: {:?}", e);
            }
        });
        
        // 6. 立即返回完整信息
        Ok(ChatAsyncResult {
            request_id,
            span_id,
            session_id: request.session_id,
        })
    }

    async fn subscribe_chat_stream(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError> {
        // 验证会话存在
        if !self.context.session_manager.session_exists(session_id).await {
            return Err(ApiError::session_not_found(session_id));
        }
        
        // 使用 SessionManager 的 subscribe_agent 方法
        let (history, stream) = self.context.session_manager
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
                }
            })
        }));
        
        Ok(Box::pin(merged_stream))
    }
}
