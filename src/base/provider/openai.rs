use super::{ChatMessage, LlmProvider, ChatResponseChunk, LlmConfig};
use crate::base::{ToolCall, AgentError};
use reqwest::Client;
use serde_json::{Value, json};
use tokio_stream::{Stream, StreamExt};
use tokio::sync::mpsc;
use std::pin::Pin;
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use serde::Serialize;
use crate::base::tool::traits::ToolDefinition;
use super::ProviderConfig;
use std::sync::Arc;

#[derive(Debug, Serialize)]
struct LlmChatRequest {
    model: String,
    messages: Vec<Value>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<JsonValue>>,
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

#[derive(Debug, Serialize)]
struct LlmChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

pub fn to_tool_json(tool: &ToolDefinition) -> JsonValue {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters_schema
        }
    })
}

pub fn to_tools_array(definitions: &[ToolDefinition]) -> Vec<JsonValue> {
    definitions.iter().map(|d| to_tool_json(d)).collect()
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

    // ------------------------------
    // 消息格式映射
    // ------------------------------
    fn map_messages(&self, messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                json!({
                    "role": msg.role,
                    "content": msg.content
                })
            })
            .collect()
    }

    // ------------------------------
    // 构建请求体
    // ------------------------------
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

    // ------------------------------
    // 合并流式工具调用分片
    // ------------------------------
    fn merge_tool_call_chunk(&self, buffer: &mut Vec<ToolCallBuffer>, chunk: &JsonValue) {
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

    // ------------------------------
    // 缓冲器转标准 ToolCall
    // ------------------------------
    fn buffer_to_tool_calls(&self, buffer: &[ToolCallBuffer]) -> Vec<ToolCall> {
        buffer
            .iter()
            .map(|b| ToolCall {
                id: b.id.clone(),
                name: b.name.clone(),
                arguments: JsonValue::String(b.arguments.clone()),
            })
            .collect()
    }

    // ------------------------------
    // 解析单条 SSE 数据
    // ------------------------------
    fn parse_sse_chunk(
        &self,
        json: &JsonValue,
        tool_buffer: &mut Vec<ToolCallBuffer>,
        response_id: &mut String,
    ) -> Result<Option<ChatResponseChunk>, AgentError> {
        // 读取 ID
        if response_id.is_empty() {
            if let Some(id) = json["id"].as_str() {
                *response_id = id.to_string();
            }
        }

        let choice = match json["choices"].as_array().and_then(|c| c.first()) {
            Some(c) => c,
            None => return Ok(None),
        };

        let delta = &choice["delta"];
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        // 处理工具调用分片
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            for call in tool_calls {
                self.merge_tool_call_chunk(tool_buffer, call);
            }
        }

        // 提取内容
        let reasoning_content = delta["reasoning_content"].as_str().map(|s| s.to_string());
        let content = delta["content"].as_str().map(|s| s.to_string());

        // 构建返回块
        let mut chunk = ChatResponseChunk {
            reasoning_content,
            content,
            id: response_id.clone(),
            tool_calls: None,
            finish_reason: None,
        };

        // 结束时返回完整工具调用
        if finish_reason.is_some() && !tool_buffer.is_empty() {
            chunk.tool_calls = Some(self.buffer_to_tool_calls(tool_buffer));
            chunk.finish_reason = finish_reason;
        }

        Ok(Some(chunk))
    }

    // ------------------------------
    // 按行处理 SSE 缓冲区
    // ------------------------------
    async fn process_sse_buffer(
        &self,
        buffer: &mut Vec<u8>,
        tx: &mpsc::Sender<Result<ChatResponseChunk, AgentError>>,
        response_id: &mut String,
        tool_buffer: &mut Vec<ToolCallBuffer>,
    ) {
        let mut start = 0;
        while start < buffer.len() {
            // 按换行切割
            let line_end = match buffer[start..].iter().position(|&b| b == b'\n') {
                Some(p) => start + p + 1,
                None => break,
            };

            let line = &buffer[start..line_end];
            start = line_end;

            // 修复：兼容低版本 Rust，判断空行
            let is_empty = line.iter().all(|&b| b.is_ascii_whitespace());
            if is_empty {
                continue;
            }

            // 只处理 data: 开头
            if !line.starts_with(b"data: ") {
                continue;
            }

            // 截取 data 内容
            let data = &line[6..line.len() - 1];
            if data == b"[DONE]" {
                break;
            }
            print!("{}\n ", String::from_utf8_lossy(data));
            // 解析 JSON
            let json = match serde_json::from_slice::<JsonValue>(data) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("JSON 解析失败：{e}");
                    continue;
                }
            };

            // 生成响应块
            match self.parse_sse_chunk(&json, tool_buffer, response_id) {
                Ok(Some(chunk)) => {
                    let _ = tx.send(Ok(chunk)).await;
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        }

        // 保留未处理完的字节
        *buffer = buffer[start..].to_vec();
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn config(&self) -> Arc<ProviderConfig> {
        self.config.clone()
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        // 1. 构建请求
        let request_body = self.build_request_body(messages, tools, config);
        let base_url = self.config.base_url.as_ref().ok_or_else(|| {
            AgentError::LlmError(format!("{}: base_url 未配置", self.config.name))
        })?;
        let api_key = self.config.api_key.clone();
        let url = format!("{}/chat/completions", base_url);

        // 2. 发送请求
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(format!("请求发送失败: {}", e)))?;

        // 3. 检查状态
        if !response.status().is_success() {
            let err = response.text().await.unwrap_or_default();
            return Err(AgentError::LlmError(format!("API响应失败: {}", err)));
        }

        // 4. 流式处理
        let (tx, rx) = mpsc::channel(100);
        let mut byte_stream = response.bytes_stream();
        let this = self.clone();

        tokio::spawn(async move {
            let mut buffer = Vec::new();
            let mut response_id = String::new();
            let mut tool_buffer = Vec::new();

            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(AgentError::LlmError(format!("流读取失败：{e}")))).await;
                        return;
                    }
                };
                buffer.extend_from_slice(&bytes);

                this.process_sse_buffer(
                    &mut buffer,
                    &tx,
                    &mut response_id,
                    &mut tool_buffer,
                ).await;
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}