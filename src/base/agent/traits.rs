use crate::base::tool::{ToolDefinition, Tool};
use crate::base::{AgentError, LlmConfig};
use crate::base::provider::{ChatMessage, LlmProvider};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use super::types::{AgentSpec, AgentOutputChunk};

pub trait Agent {
    fn get_tool_definitions(&self) -> Vec<ToolDefinition>;
    async fn execute(
        &self,
        user_input: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError>;
}

impl Agent for AgentSpec {
    fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.to_definition()).collect()
    }

    async fn execute(
        &self,
        user_input: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
        let messages = self.build_messages(user_input);
        super::loop_runner::run_agent_loop(self.clone(), messages, llm_provider, config.clone()).await
    }
}

impl AgentSpec {
    pub fn new(
        name: String,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        Self { 
            name, 
            system_prompt: Arc::new(system_prompt),  // 包装为 Arc
            tools,
            group: None,  // 默认值为None，保持向后兼容
        }
    }
    
    /// 创建带group的AgentSpec
    pub fn with_group(
        name: String,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
        group: Option<String>,
    ) -> Self {
        Self { 
            name, 
            system_prompt: Arc::new(system_prompt),  // 包装为 Arc
            tools,
            group: group.map(Arc::new),  // 将 Option<String> 转换为 Option<Arc<String>>
        }
    }

    fn build_messages(&self, user_input: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage::system(self.system_prompt.as_str())];  // 使用 as_str()
        messages.extend(user_input);
        messages
    }
}