//! OpenAI Provider 实现

use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode, Url};
use serde::Serialize;
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio_stream::{Stream, StreamExt};

use caelix_api::cancel::CancellationToken;
use caelix_api::error::AgentError;
use caelix_api::provider::{
    ChatMessage, ChatResponseChunk, LlmConfig, LlmProvider, ProviderConfig, TokenUsage,
};
use caelix_api::tool::{ApiToolCall, ToolCall, ToolDefinition};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RETRIES: usize = 3;

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
    /// 始终携带 {include_usage: true}，确保流式响应末尾返回 usage 块
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<Value>,
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

/// 解析 SSE 响应中的 usage 字段
///
/// 支持 OpenAI 标准响应结构：
/// ```json
/// { "usage": { "prompt_tokens": 12, "completion_tokens": 34, "total_tokens": 46 } }
/// ```
///
/// 以及 prompt_cache_details（缓存命中 token 数）和 reasoning_tokens。
fn parse_usage(json: &Value) -> Option<TokenUsage> {
    let usage = json.get("usage")?;
    if usage.is_null() {
        return None;
    }

    let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as u32;
    let completion_tokens = usage["completion_tokens"].as_u64().unwrap_or(0) as u32;
    let total_tokens = usage["total_tokens"].as_u64().unwrap_or(0) as u32;

    // reasoning_tokens：部分模型（Claude/DeepSeek）会在 usage 中给出
    let reasoning_tokens = usage["reasoning_tokens"].as_u64().map(|v| v as u32);

    // cache_hit_tokens：OpenAI prompt_cache_details 中的 tokens 累积
    let cache_hit_tokens = usage
        .get("prompt_cache_details")
        .and_then(|d| d.as_array())
        .and_then(|arr| {
            let mut total: u32 = 0;
            for item in arr {
                if let Some(v) = item["tokens"].as_u64() {
                    total = total.saturating_add(v as u32);
                }
            }
            if total == 0 { None } else { Some(total) }
        })
        .or_else(|| {
            // 兼容字段名 cache_hit_tokens / cached_tokens
            usage["cache_hit_tokens"]
                .as_u64()
                .map(|v| v as u32)
                .or_else(|| usage["cached_tokens"].as_u64().map(|v| v as u32))
        });

    // 如果基础字段全为 0 且额外字段也为空，则视为无 usage 信息
    if prompt_tokens == 0
        && completion_tokens == 0
        && total_tokens == 0
        && reasoning_tokens.is_none()
        && cache_hit_tokens.is_none()
    {
        return None;
    }

    Some(TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        reasoning_tokens,
        cache_hit_tokens,
    })
}

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    client: Client,
    config: Arc<ProviderConfig>,
}

impl OpenAIProvider {
    pub fn new(config: Arc<ProviderConfig>) -> Self {
        Self {
            client: shared_client().clone(),
            config,
        }
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

                serde_json::to_value(llm_msg).unwrap_or_else(|e| {
                    tracing::error!(error = %e, "LlmChatMessage serialization failed");
                    serde_json::Value::Null
                })
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
            max_tokens: self.config.max_output_tokens.or(self.config.max_tokens),
            stream: true,
            stream_options: Some(json!({ "include_usage": true })),
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
        print!("[DEBUG][openai] tool call buffer: {:#?}", buffer);

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

        let usage = parse_usage(json);

        let choices = match json["choices"].as_array() {
            Some(c) => c,
            None => {
                if usage.is_some() {
                    return Ok(Some(ChatResponseChunk {
                        reasoning_content: None,
                        content: None,
                        id: response_id.clone(),
                        tool_calls: None,
                        finish_reason: None,
                        usage,
                    }));
                }
                return Ok(None);
            }
        };

        let choice = match choices.first() {
            Some(c) => c,
            None => {
                if usage.is_some() {
                    return Ok(Some(ChatResponseChunk {
                        reasoning_content: None,
                        content: None,
                        id: response_id.clone(),
                        tool_calls: None,
                        finish_reason: None,
                        usage,
                    }));
                }
                return Ok(None);
            }
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

        let tool_calls_delta = if !tool_buffer.is_empty() {
            Some(self.buffer_to_tool_calls(tool_buffer))
        } else {
            None
        };

        let has_content = content.as_ref().is_some_and(|s| !s.is_empty());
        let has_reasoning = reasoning_content.as_ref().is_some_and(|s| !s.is_empty());
        let has_tools = tool_calls_delta.as_ref().is_some_and(|t| !t.is_empty());
        let has_usage = usage.is_some();
        let has_finish = finish_reason.is_some();

        if !has_content && !has_reasoning && !has_tools && !has_usage && !has_finish {
            return Ok(None);
        }

        Ok(Some(ChatResponseChunk {
            reasoning_content,
            content,
            id: response_id.clone(),
            tool_calls: tool_calls_delta,
            finish_reason,
            usage,
        }))
    }

    async fn send_chat_request(
        &self,
        url: &str,
        api_key: &str,
        request_body: &LlmChatRequest,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<Response, AgentError> {
        let mut attempt = 0;

        loop {
            if let Some(token) = cancel_token
                && token.is_cancelled()
            {
                return Err(AgentError::LlmError("请求已取消".to_string()));
            }

            let send_fut = self
                .client
                .post(url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(request_body)
                .send();

            let send_result = match cancel_token {
                Some(token) => {
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => return Err(AgentError::LlmError("请求已取消".to_string())),
                        result = tokio::time::timeout(SEND_TIMEOUT, send_fut) => result,
                    }
                }
                None => tokio::time::timeout(SEND_TIMEOUT, send_fut).await,
            };

            let response = match send_result {
                Ok(Ok(response)) => response,
                Ok(Err(err)) => {
                    if attempt + 1 < MAX_RETRIES && err.is_timeout() {
                        attempt += 1;
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(AgentError::LlmError(format!("请求发送失败: {}", err)));
                }
                Err(_) => {
                    if attempt + 1 < MAX_RETRIES {
                        attempt += 1;
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(AgentError::LlmError(format!(
                        "请求发送超时: {}s",
                        SEND_TIMEOUT.as_secs()
                    )));
                }
            };

            if should_retry_status(response.status()) && attempt + 1 < MAX_RETRIES {
                attempt += 1;
                retry_delay(attempt).await;
                continue;
            }

            return Ok(response);
        }
    }
}

fn shared_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();

    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .build()
            .expect("reqwest Client builder should succeed")
    })
}

fn should_retry_status(status: StatusCode) -> bool {
    status.is_server_error()
}

async fn retry_delay(attempt: usize) {
    let millis = 200_u64.saturating_mul(1_u64 << attempt.min(4));
    tokio::time::sleep(Duration::from_millis(millis)).await;
}

fn validated_base_url(raw: &str) -> Result<String, AgentError> {
    let parsed =
        Url::parse(raw).map_err(|e| AgentError::LlmError(format!("base_url 无效: {}", e)))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(AgentError::LlmError(format!(
                "base_url scheme '{}' 不被允许，仅支持 http/https",
                scheme
            )));
        }
    }

    if parsed.host_str().is_none() {
        return Err(AgentError::LlmError(
            "base_url 必须包含有效 host".to_string(),
        ));
    }

    Ok(raw.trim_end_matches('/').to_string())
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
        self.chat_stream_with_cancel(messages, tools, config, None)
            .await
    }

    async fn chat_stream_with_cancel(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>> {
        let self_clone = self.clone();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        let config = config.clone();

        let stream = async_stream::stream! {
            // 1. 构建请求（错误直接 yield）
            let request_body = self_clone.build_request_body(&messages, &tools, &config);
            let base_url = match self_clone.config.base_url.as_ref() {
                Some(url) => match validated_base_url(url) {
                    Ok(url) => url,
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                },
                None => {
                    yield Err(AgentError::LlmError(format!("{}: base_url 未配置", self_clone.config.name)));
                    return;
                }
            };

            let api_key = self_clone.config.api_key.clone();
            let url = format!("{}/chat/completions", base_url);

            // 2. 发送请求
            let response = match self_clone
                .send_chat_request(&url, &api_key, &request_body, cancel_token.as_ref())
                .await
            {
                Ok(res) => res,
                Err(e) => {
                    yield Err(e);
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

            loop {
                let next_chunk = match cancel_token.as_ref() {
                    Some(token) => {
                        tokio::select! {
                            biased;
                            _ = token.cancelled() => {
                                yield Err(AgentError::LlmError("请求已取消".to_string()));
                                return;
                            }
                            result = tokio::time::timeout(STREAM_CHUNK_TIMEOUT, byte_stream.next()) => result,
                        }
                    }
                    None => tokio::time::timeout(STREAM_CHUNK_TIMEOUT, byte_stream.next()).await,
                };

                let Some(chunk_result) = (match next_chunk {
                    Ok(chunk) => chunk,
                    Err(_) => {
                        yield Err(AgentError::LlmError(format!(
                            "流读取超时: {}s",
                            STREAM_CHUNK_TIMEOUT.as_secs()
                        )));
                        return;
                    }
                }) else {
                    break;
                };

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
                        return;
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
