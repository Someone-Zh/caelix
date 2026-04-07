use super::{ChatMessage, LlmProvider, ChatResponse, ChatResponseChunk, LlmConfig};
use crate::base::ToolCall;
use crate::base::AgentError;
use reqwest::{Client };
use serde_json::{Value, json};
use tokio_stream::{Stream, StreamExt};
use tokio::sync::mpsc;
use std::pin::Pin;
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use serde::{Serialize};
use crate::base::tool::traits::ToolDefinition;

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
    /// Raw reasoning content from thinking models; pass-through for providers
    /// that require it in assistant tool-call history messages.
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
/// 将 Vec<ToolDefinition> 批量转为 serde_json::Value 数组
pub fn to_tools_array(definitions: &[ToolDefinition]) -> Vec<JsonValue> {
    definitions.iter().map(|d| to_tool_json(d)).collect()
}

/// OpenAI 模型提供商实现
/// 
/// 负责与 OpenAI API 进行交互，处理聊天请求和流式响应
#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    /// reqwest 客户端实例
    client: Client,
    /// API 密钥
    api_key: String,
    /// API 基础 URL
    base_url: String,
}

impl OpenAIProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = Client::new();
        let base_url = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        
        Self { client, api_key, base_url }
    }

    // ------------------- 拆分方法1：消息格式映射 -------------------
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

    // ------------------- 拆分方法2：构建API请求体 -------------------
    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: LlmConfig,
    ) -> LlmChatRequest {
        LlmChatRequest {
            model: config.model_name.to_string(),
            messages: self.map_messages(messages),
            temperature: config.temperature,
            tools: Some(to_tools_array(tools)),
            tool_choice: None,
            max_tokens: None,
            stream: true,
        }
    }

    // ------------------- 拆分方法3：合并流式工具调用分片（核心修复） -------------------
    fn merge_tool_call_chunk(&self, buffer: &mut Vec<ToolCallBuffer>, chunk: &JsonValue) {
        let index = chunk["index"].as_u64().unwrap_or(0) as u32;
        let tool_id = chunk["id"].as_str().unwrap_or_default().to_string();
        let func = &chunk["function"];
        let name = func["name"].as_str().unwrap_or_default().to_string();
        let args = func["arguments"].as_str().unwrap_or_default().to_string();

        // 按index匹配，合并参数（解决分片传输问题）
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

    // ------------------- 拆分方法4：缓冲器转通用ToolCall -------------------
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

    // ------------------- 拆分方法5：处理单条SSE JSON数据 -------------------
    fn process_sse_data(
        &self,
        data: &JsonValue,
        tool_buffer: &mut Vec<ToolCallBuffer>,
        response_id: &mut String,
    ) -> Result<Option<ChatResponseChunk>, AgentError> {
        *response_id = data["id"].as_str().unwrap_or_default().to_string();
        let choice = match data["choices"].as_array().and_then(|c| c.first()) {
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

        // 处理文本内容
        let content = delta["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // 流式返回文本chunk
        if content.is_some() {
            return Ok(Some(ChatResponseChunk {
                content,
                id: response_id.clone(),
                tool_calls: None,
                finish_reason: None,
            }));
        }

        // 结束时返回完整工具调用
        if finish_reason.is_some() && !tool_buffer.is_empty() {
            return Ok(Some(ChatResponseChunk {
                content: None,
                id: response_id.clone(),
                tool_calls: Some(self.buffer_to_tool_calls(tool_buffer)),
                finish_reason,
            }));
        }

        Ok(None)
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    /// 流式聊天接口
    /// 向 OpenAI API 发送聊天请求并返回流式响应
    /// # 参数
    /// - `messages`: 聊天消息历史
    /// - `config`: LLM 配置参数
    /// 
    /// # 返回值
    /// 流式响应的结果流
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        // 1. 构建请求
        let request_body = self.build_request_body(messages, _tools, config);
        let url = format!("{}/chat/completions", self.base_url);

        // 2. 发送API请求
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(format!("请求发送失败: {}", e)))?;

        // 3. 校验响应状态
        if !response.status().is_success() {
            let err = response
                .text()
                .await
                .unwrap_or_default();
            return Err(AgentError::LlmError(format!("API响应失败: {}", err)));
        }

        // 4. 创建流式通道
        let (tx, rx) = mpsc::channel(100);
        let mut byte_stream = response.bytes_stream();
        let mut buffer = Vec::new();
        let mut tool_buffer = Vec::new();
        let mut response_id = String::new();
        let this = self.clone();
        // 5. 异步处理SSE流
        tokio::spawn(async move {
            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(AgentError::LlmError(format!("流读取失败: {}", e)))).await;
                        return;
                    }
                };
                buffer.extend_from_slice(&bytes);

                // 解析SSE行
                let mut start = 0;
                while start < buffer.len() {
                    // 查找换行符分割行
                    let line_end = match buffer[start..].iter().position(|&b| b == b'\n') {
                        Some(pos) => start + pos + 1,
                        None => break,
                    };

                    let line = &buffer[start..line_end];
                    start = line_end;

                    // 跳过空行
                    if line.len() <= 1 {
                        continue;
                    }

                    // 解析 data: 前缀
                    if !line.starts_with(b"data: ") {
                        continue;
                    }
                    let data = &line[6..line.len() - 1]; // 去除data: 和换行符

                    // 结束标志
                    if data == b"[DONE]" {
                        continue;
                    }

                    // 解析JSON并处理
                    match serde_json::from_slice::<JsonValue>(data) {
                        Ok(json) => {
                            match this.process_sse_data(&json, &mut tool_buffer, &mut response_id) {
                                Ok(Some(chunk)) => {
                                    if tx.send(Ok(chunk)).await.is_err() {
                                        return;
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(AgentError::LlmError(format!("JSON解析失败: {}", e)))).await;
                            return;
                        }
                    }
                }

                // 保留未处理的字节
                buffer = buffer[start..].to_vec();
            }
        });
        // 将通道接收器转换为流并返回
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
}