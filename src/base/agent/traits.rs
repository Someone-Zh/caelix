use crate::base::tool::traits::ToolDefinition;
use crate::base::{AgentError, LlmConfig, Tool};
use crate::base::provider::{ChatMessage, LlmProvider};
use tokio_stream::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug)]
pub struct AgentSpec {
    pub name: String,
    pub system_prompt: String,
    pub tools: Vec<Arc<dyn Tool>>,
}

impl AgentSpec {
    pub fn new(
        name: String,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {

        Self {
            name,
            system_prompt,
            tools: tools,
        }
    }
    
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.to_definition()).collect()
    }
    
    /// 执行智能体
    pub async fn execute(
        &self,
        user_input: Vec<ChatMessage>,
        llm_provider: &Box<dyn LlmProvider>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 组装 LLM 输入
        let messages = self.build_messages(user_input);

        // 执行思考-行动-观察循环
        self.run_think_act_observe_loop(messages, llm_provider).await
    }
    
    /// 构建 LLM 输入消息
    fn build_messages(&self, user_input: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut messages = vec![];

        // 添加系统提示
        messages.push(ChatMessage::system(self.system_prompt.clone()));

        // 添加用户输入
        messages.extend(user_input);

        messages
    }
    
    /// 运行思考-行动-观察循环
    async fn run_think_act_observe_loop(
        &self,
        messages: Vec<ChatMessage>,
        llm_provider: &Box<dyn LlmProvider>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 配置 LLM
        let config = LlmConfig {
            temperature: 0.7,
            max_tokens: Some(1000),
            model_name: "gpt-4".to_string(),
        };
        // 执行 LLM 调用
        let chat_stream = llm_provider.chat_stream(&messages, &self.get_tool_definitions(), config).await?;
        
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