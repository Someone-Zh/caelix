
use crate::base::agent::traits::AgentSpec;
use crate::base::agent::manager::AgentManager;

/// 智能体执行器，负责执行智能体的思考-行动-观察循环
#[derive(Debug)]
pub struct AgentExecutor {
    registry: AgentManager,
    llm_provider: Box<dyn LlmProvider>,
}

impl AgentExecutor {
    /// 创建新的智能体执行器
    pub fn new(registry: AgentManager, llm_provider: Box<dyn LlmProvider>) -> Self {
        Self {
            registry,
            llm_provider,
        }
    }

}