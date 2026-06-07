use std::{pin::Pin, sync::Arc};

use caelix_api::{Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig};
use futures::Stream;

use crate::loop_agent::LoopAgent;


pub async fn run_agent(agent_spec: Arc<AgentSpec>,
    messages: Vec<ChatMessage> ,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig) -> Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send + 'static>> {
    let agnet = LoopAgent::new(agent_spec);
    agnet.run(messages, provider, config).await
}