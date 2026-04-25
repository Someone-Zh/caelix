use std::sync::Arc;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use crate::api::CaelixApi;
use crate::api::types::{ApiError, ChatRequest, SessionSummary, ProviderInfo};
use crate::config::CaelixContext;
use crate::base::agent::{AgentOutputChunk, Agent};
use crate::base::provider::{ChatMessage, LlmConfig, LlmType};
use crate::runtime::message::agent_message::{AgentMessage, AgentMessageType};
use crate::runtime::message::notification_message::{NotificationMessage, NotificationType};
use crate::runtime::TaskMeta;
use crate::enhancement::hooks::{BaseContext, InitContext, PreContext, PostContext, ErrorContext, HookStage};
use uuid::Uuid;

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
    pub fn message_bus(&self) -> &Arc<crate::runtime::message::MessageBus> {
        &self.context.message_bus
    }
}

#[async_trait]
impl CaelixApi for CaelixApiImpl {
    fn get_default_provider(&self) -> String {
        // 从配置中获取第一个可用的提供者作为默认
        // 这里简化处理，实际应该从配置文件读取
        "bailian".to_string()
    }

    fn get_default_model(&self) -> String {
        // 从配置中获取默认模型
        // 这里简化处理，实际应该从配置文件读取
        "qwen-max".to_string()
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

    fn create_session(&self) -> String {
        // 生成 UUID 作为 session_id
        let session_id = uuid::Uuid::new_v4().to_string();
        // 在 runtime SessionManager 中创建配置
        let ctx = self.context.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            let _ = ctx.session_manager.create_session_config(session_id_clone).await;
        });
        session_id
    }

    async fn list_agents(&self) -> Vec<String> {
        let agents = self.context.agent_manager.get_all().await;
        agents.iter().map(|a| a.name.clone()).collect()
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<AgentOutputChunk, ApiError>>, ApiError> {
        // 验证会话是否存在
        if !self.context.session_manager.session_exists(&request.session_id).await {
            return Err(ApiError::session_not_found(&request.session_id));
        }

        // 获取会话配置
        let session_config = self.context.session_manager
            .get_session_config(&request.session_id)
            .await
            .ok_or_else(|| ApiError::session_not_found(&request.session_id))?;

        // 确定使用的提供者、模型和 agent
        let provider_name = request.provider
            .or(session_config.provider)
            .unwrap_or_else(|| self.get_default_provider());
        
        let model_name = request.model
            .or(session_config.model)
            .unwrap_or_else(|| self.get_default_model());
        
        let agent_name = request.agent
            .or(session_config.agent)
            .unwrap_or_else(|| "code_executor_agent".to_string());

        // 生成唯一的 stream_id（暂时未使用，为将来扩展预留）
        let _stream_id = Uuid::new_v4().to_string();
        
        // 生成 request_id（每次请求唯一）
        let request_id = Uuid::new_v4().to_string();
        
        // 克隆必要的引用用于后台任务
        let context = self.context.clone();
        let session_id = request.session_id.clone();
        let message_content = request.message.clone();
        let bus = self.context.message_bus.clone();
        let request_id = request_id.clone();
        
        // 在后台执行 agent 并通过消息总线推送流式内容
        tokio::spawn(async move {
            // 发送开始消息
            let start_span_id = AgentMessage::generate_span_id();
            let start_msg = AgentMessage {
                session_id: session_id.clone(),
                span_id: start_span_id.clone(),
                r#type: AgentMessageType::Chunk,
                timestamp: chrono::Utc::now(),
                content: "开始处理...".to_string(),
            };
            let _ = bus.send_agent(start_msg);
            
            // 获取提供者
            let provider_manager = context.llm_provider_manager.read().await;
            let provider = match provider_manager.get_provider(&provider_name) {
                Some(p) => p.clone(),
                None => {
                    // 发送错误消息
                    let error_msg = NotificationMessage {
                        session_id: session_id.clone(),
                        span_id: NotificationMessage::generate_span_id(),
                        r#type: NotificationType::Error,
                        timestamp: chrono::Utc::now(),
                        content: format!("Provider '{}' not found", provider_name),
                    };
                    let _ = bus.send_notification(error_msg);
                    return;
                }
            };

            // 获取 agent
            let agent_spec = match context.agent_manager.get(&agent_name).await {
                Some(spec) => spec,
                None => {
                    // 发送错误消息
                    let error_msg = NotificationMessage {
                        session_id: session_id.clone(),
                        span_id: NotificationMessage::generate_span_id(),
                        r#type: NotificationType::Error,
                        timestamp: chrono::Utc::now(),
                        content: format!("Agent '{}' not found", agent_name),
                    };
                    let _ = bus.send_notification(error_msg);
                    return;
                }
            };

            // 克隆agent_spec用于初始化
            let mut enhanced_agent = (*agent_spec).clone();

            // 构建BaseContext
            let base_ctx = BaseContext {
                session_id: session_id.clone(),
                request_id: request_id.clone(),
                span_id: start_span_id.clone(),
                agent_name: agent_name.clone(),
                agent_group: enhanced_agent.group.clone(),
            };

            // 执行Init钩子
            let mut init_ctx = InitContext {
                base: base_ctx.clone(),
                agent_spec: &mut enhanced_agent,
            };

            if let Err(e) = context.hook_registry.execute_init(&mut init_ctx).await {
                // 发送错误消息并返回
                let error_msg = NotificationMessage {
                    session_id: session_id.clone(),
                    span_id: NotificationMessage::generate_span_id(),
                    r#type: NotificationType::Error,
                    timestamp: chrono::Utc::now(),
                    content: format!("Init hook failed: {:?}", e),
                };
                let _ = bus.send_notification(error_msg);
                return;
            }

            // 构建消息
            let mut messages = vec![
                ChatMessage::user(message_content),
            ];

            // 执行Pre钩子
            let mut pre_ctx = PreContext {
                base: base_ctx.clone(),
                messages: messages.clone(),
            };

            if let Err(e) = context.hook_registry.execute_pre(&mut pre_ctx).await {
                // 发送错误消息并返回
                let error_msg = NotificationMessage {
                    session_id: session_id.clone(),
                    span_id: NotificationMessage::generate_span_id(),
                    r#type: NotificationType::Error,
                    timestamp: chrono::Utc::now(),
                    content: format!("Pre-process hook failed: {:?}", e),
                };
                let _ = bus.send_notification(error_msg);
                return;
            }

            // 使用修改后的messages
            messages = pre_ctx.messages;
            let input_messages_for_post = messages.clone();  // 克隆用于Post钩子

            // 构建 LLM 配置
            let config = LlmConfig {
                model_name,
            };

            // 执行 agent 并获取流
            let stream = match enhanced_agent.execute(messages, provider, &config).await {
                Ok(s) => s,
                Err(e) => {
                    // 执行Error钩子
                    let error_ctx = ErrorContext {
                        base: base_ctx.clone(),
                        error: anyhow::anyhow!("Agent execution failed: {:?}", e),
                        stage: HookStage::Pre,
                    };
                    let _ = context.hook_registry.execute_error(&error_ctx).await;
                    
                    // 发送错误消息
                    let error_msg = NotificationMessage {
                        session_id: session_id.clone(),
                        span_id: NotificationMessage::generate_span_id(),
                        r#type: NotificationType::Error,
                        timestamp: chrono::Utc::now(),
                        content: format!("Agent execution failed: {:?}", e),
                    };
                    let _ = bus.send_notification(error_msg);
                    return;
                }
            };

            // 逐块推送流式内容
            let mut _chunk_count = 0u64;
            let mut _full_content = String::new();
            let mut all_chunks = Vec::new();  // 收集所有chunks用于Post钩子
            
            let mut stream = stream;
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        all_chunks.push(chunk.clone());  // 收集chunk
                        match chunk {
                            AgentOutputChunk::Content { content } => {
                                _full_content.push_str(&content);
                                _chunk_count += 1;
                                
                                // 发送流式内容块
                                let chunk_msg = AgentMessage {
                                    session_id: session_id.clone(),
                                    span_id: AgentMessage::generate_span_id(),
                                    r#type: AgentMessageType::Chunk,
                                    timestamp: chrono::Utc::now(),
                                    content: content.clone(),
                                };
                                let _ = bus.send_agent(chunk_msg);
                            }
                            AgentOutputChunk::ToolCall { name, arguments, .. } => {
                                // 发送工具调用通知
                                let tool_msg = NotificationMessage {
                                    session_id: session_id.clone(),
                                    span_id: NotificationMessage::generate_span_id(),
                                    r#type: NotificationType::Info,
                                    timestamp: chrono::Utc::now(),
                                    content: format!("调用工具: {}({})", name, arguments),
                                };
                                let _ = bus.send_notification(tool_msg);
                            }
                            AgentOutputChunk::Finish { .. } => {
                                // 发送结束标记
                                let finish_msg = AgentMessage {
                                    session_id: session_id.clone(),
                                    span_id: AgentMessage::generate_span_id(),
                                    r#type: AgentMessageType::ChunkEnd,
                                    timestamp: chrono::Utc::now(),
                                    content: String::new(),
                                };
                                let _ = bus.send_agent(finish_msg);
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        // 执行Error钩子
                        let error_ctx = ErrorContext {
                            base: base_ctx.clone(),
                            error: anyhow::anyhow!("Stream error: {:?}", e),
                            stage: HookStage::Post,
                        };
                        let _ = context.hook_registry.execute_error(&error_ctx).await;
                        
                        // 发送错误消息
                        let error_msg = NotificationMessage {
                            session_id: session_id.clone(),
                            span_id: NotificationMessage::generate_span_id(),
                            r#type: NotificationType::Error,
                            timestamp: chrono::Utc::now(),
                            content: format!("Stream error: {:?}", e),
                        };
                        let _ = bus.send_notification(error_msg);
                        break;
                    }
                }
            }
            
            // 执行Post钩子
            let post_ctx = PostContext {
                base: base_ctx.clone(),
                input_messages: input_messages_for_post,
                output_chunks: all_chunks,
            };

            if let Err(e) = context.hook_registry.execute_post(&post_ctx).await {
                eprintln!("Post-process hook failed: {:?}", e);
                // Post钩子失败不影响已发送的内容
            }
        });

        // 立即返回一个空的已完成流，告知客户端任务已在后台执行
        // 实际内容会通过消息总线推送
        let empty_stream = futures::stream::empty();
        Ok(Box::pin(empty_stream))
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
        Ok(task_manager.list_tasks(session_id).await)
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
                        let chars: Vec<char> = msg.content.chars().collect();
                        if chars.len() > 15 {
                            chars[..15].iter().collect::<String>() + "..."
                        } else {
                            msg.content.clone()
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
                    LlmType::OpenAI => "openai".to_string(),
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
}
