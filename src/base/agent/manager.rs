use std::collections::HashMap;
use tokio_stream::{Stream, StreamExt};
use crate::base::{AgentError, LlmConfig};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::base::agent::traits::AgentSpec;
use crate::base::provider::ChatMessage;
use crate::base::provider::LlmProvider;
use std::pin::Pin;


/// 智能体注册中心，负责维护所有智能体蓝图的索引
#[derive(Debug, Clone)]
pub struct AgentManager {
    agents: Arc<RwLock<HashMap<String, Arc<AgentSpec>>>>,
}

impl AgentManager {
    /// 创建新的智能体注册中心
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册智能体蓝图
    pub async fn register(&self, agent_spec: AgentSpec) -> Result<(), AgentRegistryError> {
        let mut agents = self.agents.write().await;
        if agents.contains_key(&agent_spec.name) {
            return Err(AgentRegistryError::AgentAlreadyExists(agent_spec.name));
        }
        agents.insert(agent_spec.name.clone(), Arc::new(agent_spec));
        Ok(())
    }

    /// 根据名称获取智能体蓝图
    pub async fn get(&self, name: &str) -> Option<Arc<AgentSpec>> {
        let agents = self.agents.read().await;
        agents.get(name).cloned()
    }

    /// 获取所有智能体蓝图
    pub async fn get_all(&self) -> Vec<Arc<AgentSpec>> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// 移除智能体蓝图
    pub async fn remove(&self, name: &str) -> Option<Arc<AgentSpec>> {
        let mut agents = self.agents.write().await;
        agents.remove(name)
    }



    /// 执行智能体
    pub async fn execute(
        &self,
        agent_name: &str,
        user_input: Vec<ChatMessage>,
        llm_provider: &Box<dyn LlmProvider>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 从注册中心获取智能体蓝图
        let agent_spec = self.get(agent_name).await
            .ok_or(AgentError::AgentNotFound(agent_name.to_string()))?;

        // 组装 LLM 输入
        let messages = self.build_messages(&agent_spec, user_input);

        // 执行思考-行动-观察循环
        self.run_think_act_observe_loop(messages, &*agent_spec, llm_provider).await
    }

    /// 构建 LLM 输入消息
    fn build_messages(&self, agent_spec: &AgentSpec, user_input: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut messages = vec![];

        // 添加系统提示
        messages.push(ChatMessage::system(agent_spec.system_prompt.clone()));

        // 添加用户输入
        messages.extend(user_input);

        messages
    }

    /// 运行思考-行动-观察循环
    async fn run_think_act_observe_loop(
        &self,
        messages: Vec<ChatMessage>,
        agent_spec: &AgentSpec,
        llm_provider: &Box<dyn LlmProvider>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 配置 LLM
        let config = LlmConfig {
            temperature: 0.7,
            max_tokens: Some(1000),
            model_name: "gpt-4".to_string(),
        };
        // 执行 LLM 调用
        let chat_stream = llm_provider.chat_stream(&messages, &agent_spec.get_tool_definitions(), config).await?;
        
        // 将 ChatResponseChunk 流转换为 String 流
        let string_stream = Box::pin(chat_stream.map(|result| {
            match result {
                Ok(chunk) => {
                    match chunk.content {
                        Some(content) => Ok(content),
                        None => Ok("".to_string()),
                    }
                }
                Err(e) => Err(e),
            }
        }));

        Ok(string_stream)
    }
}

/// 智能体注册中心错误
#[derive(Debug, thiserror::Error)]
pub enum AgentRegistryError {
    #[error("Agent with name '{0}' already exists")]
    AgentAlreadyExists(String),
}