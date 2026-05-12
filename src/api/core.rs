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
use crate::runtime::message::notification_message::NotificationMessage;
use crate::runtime::TaskMeta;

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

    async fn create_session(&self) -> String {
        // 使用中央ID生成器生成 session_id
        let session_id = crate::runtime::id_generator::generate_session_id();
        // 在 runtime SessionManager 中创建配置（等待完成）
        if let Err(e) = self.context.session_manager
            .create_session_config(session_id.clone())
            .await
        {
            eprintln!("⚠️  创建会话配置失败: {:?}", e);
        }
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
        // 1. 如果会话不存在则创建
        if !self.context.session_manager.session_exists(&request.session_id).await {
            self.context.session_manager
                .create_session_config(request.session_id.clone())
                .await
                .map_err(|e| ApiError::InternalError(e.to_string()))?;
        }
        
        // 2. 确定provider和model（用于创建RuntimeContext）
        let default_provider = self.get_default_provider();
        let default_model = self.get_default_model();
        let provider_name = request.provider.as_deref()
            .unwrap_or(&default_provider);
        let model_name = request.model.as_deref()
            .unwrap_or(&default_model);
        
        // 3. 创建 RuntimeContext
        let work_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let runtime_ctx = crate::runtime::context::RuntimeContext::new(
            Some(request.session_id.clone()),
            Some(crate::runtime::id_generator::generate_request_id()),
            work_dir,
            provider_name.to_string(),
            model_name.to_string(),
            false,
            self.context.clone(),
        );
        
        // 4. 在 RuntimeContext scope 中执行所有操作
        let ctx_clone = self.context.clone();
        let request_clone = request.clone();
        
        let result = crate::runtime::context::RuntimeContext::scope(runtime_ctx, async move {
            // 4.1 确定使用的agent名称（默认使用第一个或从请求中获取）
            let agent_name = request_clone.agent.as_deref().unwrap_or("default");
            
            // 4.2 通过agent_manager获取AgentSpec
            let agent_spec = ctx_clone.agent_manager.get(agent_name).await
                .ok_or_else(|| ApiError::agent_not_found(agent_name))?;
            
            // 4.3 通过llm_provider_manager获取对应的Provider实例
            let provider = {
                let provider_manager = ctx_clone.llm_provider_manager.read().await;
                provider_manager.get_provider(provider_name)
                    .ok_or_else(|| ApiError::provider_not_found(provider_name))?
                    .clone()
            };
            
            // 4.4 构建LlmConfig
            let config = LlmConfig {
                model_name: model_name.to_string(),
            };
            
            // 4.5 构建消息列表（从会话历史 + 当前消息）
            // AgentMessage.content 现在存储的是 ChatMessage 的 JSON 字符串
            let history_messages = ctx_clone.session_manager
                .get_session_messages(&request_clone.session_id)
                .await
                .map_err(|e| ApiError::InternalError(e.to_string()))?;
            
            let mut messages: Vec<ChatMessage> = Vec::new();
            for msg in history_messages.iter() {
                if msg.r#type == AgentMessageType::Msg {
                    // 尝试从 JSON 字符串反序列化为 ChatMessage
                    match serde_json::from_str::<ChatMessage>(&msg.content) {
                        Ok(chat_msg) => {
                            // 成功反序列化，使用完整的 ChatMessage
                            messages.push(chat_msg);
                        }
                        Err(_) => {
                            // 降级处理：如果不是 JSON 格式，则当作纯文本处理
                            // 这种情况可能是旧数据或手动发送的简单消息
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
                request_id: crate::runtime::id_generator::generate_request_id(),
                span_id: crate::runtime::id_generator::generate_span_id(),
                r#type: AgentMessageType::Msg,
                timestamp: chrono::Utc::now(),
                content: request_clone.message.clone(),
            };
            let _ = ctx_clone.message_bus.send_agent(user_msg);
            
            // 4.6 调用AgentSpec的execute方法获取流
            let stream = agent_spec.execute(messages, provider, &config).await?;
            
            // 4.7 转换流类型: AgentError -> ApiError
            let converted_stream = stream.map(|result| {
                result.map_err(ApiError::from)
            });
            
            Ok(Box::pin(converted_stream) as BoxStream<'static, Result<AgentOutputChunk, ApiError>>)
        }).await;
        
        result
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
