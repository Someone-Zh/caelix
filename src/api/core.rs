use std::sync::Arc;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use crate::api::CaelixApi;
use crate::api::types::{ApiError, ChatRequest, SessionSummary, ProviderInfo};
use crate::config::CaelixContext;
use crate::base::agent::{AgentOutputChunk, Agent};
use crate::base::provider::{ChatMessage, LlmConfig, LlmType};
use crate::runtime::message::types::Message;
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

        // 获取提供者
        let provider_manager = self.context.llm_provider_manager.read().await;
        let provider = provider_manager
            .get_provider(&provider_name)
            .ok_or_else(|| ApiError::provider_not_found(&provider_name))?
            .clone();

        // 获取 agent
        let agent_spec = self.context.agent_manager
            .get(&agent_name)
            .await
            .ok_or_else(|| ApiError::agent_not_found(&agent_name))?;

        // 应用钩子增强 AgentSpec
        let mut enhanced_agent = (*agent_spec).clone();
        self.context.hook_registry.apply_hooks(&mut enhanced_agent).await;

        // 构建消息
        let messages = vec![
            ChatMessage::user(request.message),
        ];

        // 构建 LLM 配置
        let config = LlmConfig {
            model_name,
        };

        // 执行 agent 并获取流
        let stream: BoxStream<'_, Result<AgentOutputChunk, _>> = match enhanced_agent.execute(messages, provider, &config).await {
            Ok(s) => Box::pin(s),
            Err(e) => {
                return Err(ApiError::InternalError(format!("Agent execution failed: {:?}", e)));
            }
        };

        // 转换流类型
        let transformed_stream = stream.map(|result| {
            result.map_err(|e| ApiError::StreamError(format!("{:?}", e)))
        });

        Ok(Box::pin(transformed_stream))
    }

    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<Message>, ApiError> {
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

    async fn get_session_notifications(&self, session_id: &str) -> Result<Vec<Message>, ApiError> {
        // 验证会话存在
        if !self.context.session_manager.session_exists(session_id).await {
            return Err(ApiError::session_not_found(session_id));
        }
        
        // 从 SessionManager 获取通知消息
        self.context.session_manager
            .get_session_notifications(session_id)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))
    }
}
