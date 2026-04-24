use std::sync::Arc;
use futures::Stream;
use std::pin::Pin;

use crate::config::CaelixContext;
use crate::base::agent::{AgentOutputChunk, Agent};
use crate::base::provider::{ChatMessage, LlmConfig};
use crate::base::AgentError;

/// Runner - 统一的Agent执行器
/// 
/// 提供统一对外的执行agent的方法，内部管理钩子的执行时机
#[allow(dead_code)] // 公共API，供外部使用
pub struct Runner {
    /// 运行时上下文引用
    context: Arc<CaelixContext>,
}

impl Runner {
    #[allow(dead_code)] // 公共API，供外部使用
    /// 创建新的Runner实例
    pub fn new(context: Arc<CaelixContext>) -> Self {
        Self { context }
    }

    /// 执行Agent
    /// 
    /// # Arguments
    /// * `agent_name` - Agent名称
    /// * `provider_name` - Provider名称
    /// * `model_name` - 模型名称
    /// * `messages` - 输入消息列表
    /// 
    /// # Returns
    /// 返回Agent输出流
    #[allow(dead_code)] // 公共API，供外部使用
    pub async fn execute_agent(
        &self,
        agent_name: &str,
        provider_name: &str,
        model_name: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>,
        Box<dyn std::error::Error>,
    > {
        // 1. 通过agent_manager获取AgentSpec
        let agent_spec = self.context.agent_manager.get(agent_name).await
            .ok_or_else(|| format!("Agent '{}' not found", agent_name))?;

        // 2. 通过llm_provider_manager获取对应的Provider实例
        let provider = {
            let provider_manager = self.context.llm_provider_manager.read().await;
            provider_manager.get_provider(provider_name)
                .ok_or_else(|| format!("Provider '{}' not found", provider_name))?
                .clone()  // 克隆Arc以获取所有权
        };

        // 3. 构建LlmConfig
        let config = LlmConfig {
            model_name: model_name.to_string(),
        };

        // 4. 调用AgentSpec的execute方法
        // 注意：init-hooks已在注册时应用，这里不需要再次增强
        let result = agent_spec.execute(messages, provider, &config).await?;

        Ok(result)
    }

    /// 执行Agent并收集所有输出
    /// 
    /// # Arguments
    /// * `agent_name` - Agent名称
    /// * `provider_name` - Provider名称
    /// * `model_name` - 模型名称
    /// * `messages` - 输入消息列表
    /// 
    /// # Returns
    /// 返回所有输出的集合
    #[allow(dead_code)] // 公共API，供外部使用
    pub async fn execute_and_collect(
        &self,
        agent_name: &str,
        provider_name: &str,
        model_name: &str,
        messages: Vec<ChatMessage>,
    ) -> Result<Vec<AgentOutputChunk>, Box<dyn std::error::Error>> {
        use futures::StreamExt;

        let mut stream = self.execute_agent(agent_name, provider_name, model_name, messages).await?;
        
        let mut outputs = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => outputs.push(chunk),
                Err(e) => return Err(Box::new(e)),
            }
        }

        Ok(outputs)
    }
}
