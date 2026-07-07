use caelix_api::agent::AgentOutputChunk;
use caelix_api::error::AgentError;
use caelix_api::provider::ChatResponseChunk;

/// 转换 LLM 响应分片为 Agent 输出分片
pub fn convert_chunk(chunk: ChatResponseChunk) -> Result<AgentOutputChunk, AgentError> {
    if let Some(r) = chunk.reasoning_content.filter(|s| !s.is_empty()) {
        return Ok(AgentOutputChunk::Reasoning { content: r });
    }
    if let Some(c) = chunk.content.filter(|s| !s.is_empty()) {
        return Ok(AgentOutputChunk::Content { content: c });
    }
    if let Some(r) = chunk.finish_reason {
        return Ok(AgentOutputChunk::Finish {
            reason: r,
            usage: chunk.usage,
        });
    }
    Ok(AgentOutputChunk::Content {
        content: String::new(),
    })
}
