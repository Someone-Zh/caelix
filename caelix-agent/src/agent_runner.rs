use std::sync::Arc;

use caelix_api::{Agent, AgentError, AgentOutputChunk, AgentSpec, ChatMessage, LlmConfig};
use futures::StreamExt;
use tracing::Instrument;

use crate::loop_agent::LoopAgent;
use crate::observability::ObserverContext;

/// 执行 Agent，将各类输出分片发送到消息总线，并返回累积的文本内容。
///
/// 核心流程：创建 LoopAgent → 消费流 → 累积 Content → 返回结果。
///
/// 外部观察者（消息总线转发、用量追踪）通过 [`ObserverContext`] 接入，
/// 其失败仅记录日志，不会影响核心运行流程。
pub async fn run_agent(
    agent_spec: Arc<AgentSpec>,
    messages: Vec<ChatMessage>,
    provider: Arc<dyn caelix_api::provider::LlmProvider>,
    config: &LlmConfig,
) -> Result<String, AgentError> {
    let agent_name = Some(agent_spec.name.clone());
    let agent = LoopAgent::new(agent_spec);
    let provider_name = provider.config().name.clone();
    let model_name = config.model_name.clone();
    let mut stream = agent.run(messages, provider, config).await;

    // 获取 tracing span（若 RuntimeContext 存在）
    let session_span = caelix_api::context::RuntimeContext::try_current()
        .map(|ctx| ctx.session_span());

    let fut = async move {
        tracing::info!(
            agent = agent_name.as_deref().unwrap_or(""),
            "run_agent start"
        );

        // 初始化外部观察者（消息总线、用量追踪）；若上下文未初始化则静默跳过
        let observer = ObserverContext::from_current(agent_name, provider_name, model_name);

        let mut result_content = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;

            // 外部观察者：转发到消息总线、记录用量（失败不影响核心流程）
            observer.dispatch_chunk(&chunk).await;

            // 核心：累积最终文本内容（仅 Content 类型的分片）
            if let AgentOutputChunk::Content { content } = &chunk {
                result_content.push_str(content);
            }

            // 核心：遇到 Stopped 直接中断
            if let AgentOutputChunk::Stopped { reason } = &chunk {
                tracing::info!(reason = reason.as_str(), "run_agent stopped by user");
                return Ok(result_content);
            }
        }

        tracing::info!(output_len = result_content.len(), "run_agent finished");

        Ok(result_content)
    };

    match session_span {
        Some(span) => fut.instrument(span).await,
        None => fut.await,
    }
}
