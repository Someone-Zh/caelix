use std::pin::Pin;
use tokio_stream::{Stream, StreamExt};
use crate::core::{AgentError, Message, Role, ToolCall, LlmConfig};
use crate::core::agent::spec::AgentSpec;
use crate::core::agent::registry::AgentRegistry;
use crate::core::llm::LlmProvider;

/// 智能体执行器，负责执行智能体的思考-行动-观察循环
#[derive(Debug, Clone)]
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
        user_input: Vec<Message>,
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
    fn build_messages(&self, agent_spec: &AgentSpec, user_input: Vec<Message>) -> Vec<Message> {
        let mut messages = vec![];

        // 添加系统提示
        messages.push(Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::System,
            content: agent_spec.system_prompt.clone(),
            tool_calls: vec![],
            timestamp: chrono::Utc::now().timestamp(),
        });

        // 添加用户输入
        messages.extend(user_input);

        messages
    }

    /// 运行思考-行动-观察循环
    async fn run_think_act_observe_loop(
        &self,
        mut messages: Vec<Message>,
        agent_spec: AgentSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AgentError>> + Send>>, AgentError> {
        // 配置 LLM
        let config = LlmConfig {
            temperature: 0.7,
            max_tokens: Some(1000),
            model_name: "gpt-4".to_string(),
        };

        // 执行 LLM 调用
        let stream = self.llm_provider.chat_stream(messages.clone(), config).await?;

        // 处理 LLM 响应
        // 这里简化处理，实际实现需要处理工具调用和多轮对话
        Ok(stream)
    }
}