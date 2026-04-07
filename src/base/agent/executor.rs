use std::pin::Pin;
use tokio_stream::{Stream, StreamExt};
use crate::base::{AgentError, LlmConfig};
use crate::base::llm::ChatMessage;
use crate::base::agent::traits::AgentSpec;
use crate::base::agent::registry::AgentRegistry;
use crate::base::llm::LlmProvider;
use crate::base::tool::ToolDefinition;

/// 智能体执行器，负责执行智能体的思考-行动-观察循环
#[derive(Debug)]
pub struct AgentExecutor {
    registry: AgentRegistry,
    llm_provider: Box<dyn LlmProvider>,
}

impl AgentExecutor {
    /// 创建新的智能体执行器
    pub fn new(registry: AgentRegistry, llm_provider: Box<dyn LlmProvider>) -> Self {
        Self {
            registry,
            llm_provider,
        }
    }

    /// 执行智能体
    pub async fn execute(
        &self,
        agent_name: &str,
        user_input: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 从注册中心获取智能体蓝图
        let agent_spec = self.registry.get(agent_name).await
            .ok_or(AgentError::AgentNotFound(agent_name.to_string()))?;

        // 组装 LLM 输入
        let messages = self.build_messages(&agent_spec, user_input);

        // 执行思考-行动-观察循环
        self.run_think_act_observe_loop(messages, agent_spec).await
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
        agent_spec: AgentSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 配置 LLM
        let config = LlmConfig {
            temperature: 0.7,
            max_tokens: Some(1000),
            model_name: "gpt-4".to_string(),
        };
        // 执行 LLM 调用
        let chat_stream = self.llm_provider.chat_stream(&messages, &agent_spec.tool_definitions, config).await?;
        
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