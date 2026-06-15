//! OpenAI Provider 实现

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::{Stream, StreamExt};

use caelix_api::error::AgentError;
use caelix_api::provider::{
    ChatMessage, ChatResponseChunk, LlmConfig, LlmProvider, ProviderConfig,
};
use caelix_api::tool::{ApiToolCall, ToolCall, ToolDefinition};

#[derive(Debug, Serialize)]
struct LlmChatRequest {
    model: String,
    messages: Vec<Value>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Default)]
struct ToolCallBuffer {
    index: u32,
    id: String,
    name: String,
    arguments: String,
}

/// 🔥 修复：用于发送的消息结构
#[derive(Debug, Serialize)]
struct LlmChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

pub fn to_tool_json(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters_schema
        }
    })
}

pub fn to_tools_array(definitions: &[ToolDefinition]) -> Vec<Value> {
    definitions.iter().map(to_tool_json).collect()
}

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    client: Client,
    config: Arc<ProviderConfig>,
}

impl OpenAIProvider {
    pub fn new(config: Arc<ProviderConfig>) -> Self {
        let client = Client::new();
        Self { client, config }
    }

    fn map_messages(&self, messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| {
                let api_tool_calls = m
                    .tool_calls
                    .as_ref()
                    .map(|tcs| tcs.iter().map(|tc| tc.to_api_format()).collect::<Vec<_>>());

                let llm_msg = LlmChatMessage {
                    role: m.role.clone(),
                    content: if m.content.is_empty() {
                        None
                    } else {
                        Some(m.content.clone())
                    },
                    tool_call_id: m.tool_call_id.clone(),
                    tool_calls: api_tool_calls,
                    reasoning_content: None,
                };

                serde_json::to_value(llm_msg).unwrap()
            })
            .collect()
    }

    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> LlmChatRequest {
        LlmChatRequest {
            model: config.model_name.to_string(),
            messages: self.map_messages(messages),
            temperature: self.config.temperature.unwrap_or(0.0f32),
            tools: Some(to_tools_array(tools)),
            tool_choice: None,
            max_tokens: None,
            stream: true,
        }
    }

    fn merge_tool_call_chunk(&self, buffer: &mut Vec<ToolCallBuffer>, chunk: &Value) {
        let index = chunk["index"].as_u64().unwrap_or(0) as u32;
        let tool_id = chunk["id"].as_str().unwrap_or_default().to_string();
        let func = &chunk["function"];
        let name = func["name"].as_str().unwrap_or_default().to_string();
        let args = func["arguments"].as_str().unwrap_or_default().to_string();

        if let Some(exist) = buffer.iter_mut().find(|b| b.index == index) {
            exist.arguments.push_str(&args);
            if !tool_id.is_empty() && exist.id.is_empty() {
                exist.id = tool_id;
            }
            if !name.is_empty() && exist.name.is_empty() {
                exist.name = name;
            }
        } else {
            buffer.push(ToolCallBuffer {
                index,
                id: tool_id,
                name,
                arguments: args,
            });
        }
    }

    fn buffer_to_tool_calls(&self, buffer: &[ToolCallBuffer]) -> Vec<ToolCall> {
        buffer
            .iter()
            .map(|b| ToolCall {
                id: b.id.clone(),
                index: b.index,
                name: b.name.clone(),
                arguments: Value::String(b.arguments.clone()),
                approval_state: None,
            })
            .collect()
    }

    fn parse_sse_chunk(
        &self,
        json: &Value,
        tool_buffer: &mut Vec<ToolCallBuffer>,
        response_id: &mut String,
    ) -> Result<Option<ChatResponseChunk>, AgentError> {
        if response_id.is_empty()
            && let Some(id) = json["id"].as_str()
        {
            *response_id = id.to_string();
        }

        let choice = match json["choices"].as_array().and_then(|c| c.first()) {
            Some(c) => c,
            None => return Ok(None),
        };

        let delta = &choice["delta"];
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            for call in tool_calls {
                self.merge_tool_call_chunk(tool_buffer, call);
            }
        }

        let reasoning_content = delta["reasoning_content"].as_str().map(|s| s.to_string());
        let content = delta["content"].as_str().map(|s| s.to_string());

        let mut chunk = ChatResponseChunk {
            reasoning_content,
            content,
            id: response_id.clone(),
            tool_calls: None,
            finish_reason: None,
        };

        if finish_reason.is_some() && !tool_buffer.is_empty() {
            chunk.tool_calls = Some(self.buffer_to_tool_calls(tool_buffer));
            chunk.finish_reason = finish_reason;
        }

        Ok(Some(chunk))
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn config(&self) -> Arc<ProviderConfig> {
        self.config.clone()
    }

    // ✅✅✅ 你要的最终返回值（无外层 Result）
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>> {
        let self_clone = self.clone();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        let config = config.clone();

        let stream = async_stream::stream! {
            // 1. 构建请求（错误直接 yield）
            let request_body = self_clone.build_request_body(&messages, &tools, &config);
            let base_url = match self_clone.config.base_url.as_ref() {
                Some(url) => url,
                None => {
                    yield Err(AgentError::LlmError(format!("{}: base_url 未配置", self_clone.config.name)));
                    return;
                }
            };

            let api_key = self_clone.config.api_key.clone();
            let url = format!("{}/chat/completions", base_url);

            // 2. 发送请求
            let response = match self_clone.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
            {
                Ok(res) => res,
                Err(e) => {
                    yield Err(AgentError::LlmError(format!("请求发送失败: {}", e)));
                    return;
                }
            };

            // 3. HTTP 状态错误
            if !response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                yield Err(AgentError::LlmError(format!("API 响应失败: {}", text)));
                return;
            }

            // 4. 正常流式解析
            let mut byte_stream = response.bytes_stream();
            let mut buffer = Vec::new();
            let mut response_id = String::new();
            let mut tool_buffer = Vec::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(AgentError::LlmError(format!("流读取失败: {}", e)));
                        return;
                    }
                };

                buffer.extend_from_slice(&bytes);

                let mut start = 0;
                while start < buffer.len() {
                    let line_end = match buffer[start..].iter().position(|&b| b == b'\n') {
                        Some(p) => start + p + 1,
                        None => break,
                    };

                    let line = &buffer[start..line_end];
                    start = line_end;

                    if line.iter().all(|&b| b.is_ascii_whitespace()) {
                        continue;
                    }

                    if !line.starts_with(b"data: ") {
                        continue;
                    }

                    let data = &line[6..line.len() - 1];
                    if data == b"[DONE]" {
                        break;
                    }

                    let json = match serde_json::from_slice::<Value>(data) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::warn!(error = %e, "JSON chunk parse failed");
                            continue;
                        }
                    };

                    match self_clone.parse_sse_chunk(&json, &mut tool_buffer, &mut response_id) {
                        Ok(Some(chunk)) => yield Ok(chunk),
                        Ok(None) => {},
                        Err(e) => yield Err(e),
                    };
                }

                buffer.drain(..start);
            }
        };

        Box::pin(stream)
    }
}
