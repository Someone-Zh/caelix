use super::{Message, MessageRole, ToolCall, LlmProvider, ChatResponse, ChatResponseChunk, LlmConfig};
use crate::base::AgentError;
use reqwest::{Client, Response};
use serde_json::{Value, json};
use tokio_stream::{Stream, StreamExt};
use tokio::sync::mpsc;
use std::pin::Pin;
use async_trait::async_trait;

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
    /// 模型名称映射表，用于将通用模型名映射到具体的 OpenAI 模型名
    models: std::collections::HashMap<String, String>,
}

impl OpenAIProvider {
    /// 创建新的 OpenAI 提供商实例
    /// 
    /// # 参数
    /// - `api_key`: OpenAI API 密钥
    /// - `base_url`: 可选的 API 基础 URL，用于自定义 API 端点（如 Azure OpenAI）
    /// - `models`: 模型名称映射表，用于将通用模型名映射到具体的 OpenAI 模型名
    /// 
    /// # 返回值
    /// 新创建的 OpenAIProvider 实例
    pub fn new(api_key: String, base_url: Option<String>, models: std::collections::HashMap<String, String>) -> Self {
        let client = Client::new();
        let base_url = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        
        Self {
            client,
            api_key,
            base_url,
            models,
        }
    }
    
    /// 将通用消息格式映射为 OpenAI 特定的消息格式
    /// 
    /// # 参数
    /// - `messages`: 通用消息向量
    /// 
    /// # 返回值
    /// OpenAI 格式的消息向量 (serde_json::Value)
    fn map_messages(&self, messages: Vec<Message>) -> Vec<Value> {
        messages.into_iter().map(|msg| {
            // 映射角色
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
            };
            
            // 创建 OpenAI 消息对象
            json!({
                "role": role,
                "content": msg.content
            })
        }).collect()
    }
    
    /// 将 OpenAI 工具调用格式映射为通用工具调用格式
    /// 
    /// # 参数
    /// - `tool_calls`: OpenAI 工具调用向量（可选）
    /// 
    /// # 返回值
    /// 通用格式的工具调用向量（可选）
    fn map_tool_calls(&self, tool_calls: Option<Vec<Value>>) -> Option<Vec<ToolCall>> {
        tool_calls.map(|calls| {
            calls.into_iter().map(|call| {
                ToolCall {
                    id: call["id"].as_str().unwrap_or("").to_string(),
                    name: call["function"]["name"].as_str().unwrap_or("").to_string(),
                    arguments: call["function"]["arguments"].clone(),
                }
            }).collect()
        })
    }
    
    /// 获取实际使用的模型名称
    /// 
    /// 如果在映射表中找到对应模型名，则使用映射后的名称；否则使用原始名称
    /// 
    /// # 参数
    /// - `model_name`: 模型名称
    /// 
    /// # 返回值
    /// 实际使用的模型名称
    fn get_model(&self, model_name: &str) -> String {
        self.models.get(model_name).cloned().unwrap_or(model_name.to_string())
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    /// 流式聊天接口
    /// 
    /// 向 OpenAI API 发送聊天请求并返回流式响应
    /// 
    /// # 参数
    /// - `messages`: 聊天消息历史
    /// - `config`: LLM 配置参数
    /// 
    /// # 返回值
    /// 流式响应的结果流
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        // 映射消息格式
        let openai_messages = self.map_messages(messages);
        // 获取实际使用的模型名称
        let model = self.get_model(&config.model_name);
        
        // 构建请求体
        let request_body = json!({
            "model": model,
            "messages": openai_messages,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
            "stream": true
        });
        
        // 构建请求 URL
        let url = format!("{}/chat/completions", self.base_url);
        
        // 发送请求并获取响应
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        // 检查响应状态
        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError::LlmError(e.to_string()))?;
            return Err(AgentError::LlmError(format!("API error: {}", error_text)));
        }
        
        // 创建通道用于传递流式响应
        let (tx, rx) = mpsc::channel(100);
        
        // 处理流式响应
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();
            // 收集工具调用信息
            let mut collected_tool_calls: Vec<ToolCall> = Vec::new();
            let mut has_tool_calls = false;
            let mut response_id = String::new();
            
            // 处理每个响应块
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        
                        // 处理 SSE 格式
                        let mut start = 0;
                        while start < buffer.len() {
                            // 查找换行符
                            if let Some(end) = buffer[start..].iter().position(|&b| b == b'\n') {
                                let line_end = start + end + 1;
                                let line = &buffer[start..line_end];
                                
                                // 跳过空行
                                if line.len() <= 1 {
                                    start = line_end;
                                    continue;
                                }
                                
                                // 解析 SSE 事件
                                if let Some(data_start) = line.starts_with(b"data: ").then(|| 6) {
                                    let data = &line[data_start..line.len()-1]; // 去掉换行符
                                    
                                    // 检查是否是结束事件
                                    if data == b"[DONE]" {
                                        start = line_end;
                                        continue;
                                    }
                                    
                                    // 解析 JSON
                                    match serde_json::from_slice::<Value>(data) {
                                        Ok(response) => {
                                            response_id = response["id"].as_str().unwrap_or("").to_string();
                                            
                                            if let Some(choice) = response["choices"].as_array().and_then(|c| c.first()) {
                                                // 检查是否有工具调用
                                                if let Some(tool_calls) = choice["delta"]["tool_calls"].as_array() {
                                                    has_tool_calls = true;
                                                    // 收集工具调用信息
                                                    for call in tool_calls {
                                                        collected_tool_calls.push(ToolCall {
                                                            id: call["id"].as_str().unwrap_or("").to_string(),
                                                            name: call["function"]["name"].as_str().unwrap_or("").to_string(),
                                                            arguments: call["function"]["arguments"].clone(),
                                                        });
                                                    }
                                                }
                                                
                                                // 检查是否有内容
                                                if let Some(content) = choice["delta"]["content"].as_str() {
                                                    if !content.is_empty() {
                                                        // 文字内容，每个chunk都返回
                                                        let chunk = ChatResponseChunk {
                                                            content: Some(content.to_string()),
                                                            id: response_id.clone(),
                                                            tool_calls: None,
                                                            finish_reason: choice["finish_reason"].as_str().map(|s| s.to_string()),
                                                        };
                                                        
                                                        // 发送到通道，如果通道关闭则退出
                                                        if tx.send(Ok(chunk)).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
                                                
                                                // 检查是否完成
                                                if let Some(finish_reason) = choice["finish_reason"].as_str() {
                                                    // 如果有工具调用，在完成时返回完整的工具调用信息
                                                    if has_tool_calls && !collected_tool_calls.is_empty() {
                                                        let chunk = ChatResponseChunk {
                                                            content: None,
                                                            id: response_id.clone(),
                                                            tool_calls: Some(collected_tool_calls.clone()),
                                                            finish_reason: Some(finish_reason.to_string()),
                                                        };
                                                        
                                                        // 发送到通道，如果通道关闭则退出
                                                        if tx.send(Ok(chunk)).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            // 发送错误信息并退出
                                            let _ = tx.send(Err(AgentError::LlmError(e.to_string()))).await;
                                            return;
                                        }
                                    }
                                }
                                
                                start = line_end;
                            } else {
                                // 没有找到完整的行，等待更多数据
                                break;
                            }
                        }
                        
                        // 保留未处理的数据
                        if start < buffer.len() {
                            buffer = buffer[start..].to_vec();
                        } else {
                            buffer.clear();
                        }
                    }
                    Err(e) => {
                        // 发送错误信息并退出
                        let _ = tx.send(Err(AgentError::LlmError(e.to_string()))).await;
                        return;
                    }
                }
            }
        });
        
        // 将通道接收器转换为流并返回
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    /// 非流式聊天接口
    /// 
    /// 向 OpenAI API 发送聊天请求并返回完整响应
    /// 
    /// # 参数
    /// - `messages`: 聊天消息历史
    /// - `config`: LLM 配置参数
    /// 
    /// # 返回值
    /// 完整的聊天响应
    async fn chat(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<ChatResponse, AgentError> {
        // 映射消息格式
        let openai_messages = self.map_messages(messages);
        // 获取实际使用的模型名称
        let model = self.get_model(&config.model_name);
        
        // 构建请求体
        let request_body = json!({
            "model": model,
            "messages": openai_messages,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
            "stream": false
        });
        
        // 构建请求 URL
        let url = format!("{}/chat/completions", self.base_url);
        
        // 发送请求并获取响应
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        // 检查响应状态
        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError::LlmError(e.to_string()))?;
            return Err(AgentError::LlmError(format!("API error: {}", error_text)));
        }
        
        // 解析响应
        let response_json: Value = response.json().await.map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        // 提取助手消息
        let assistant_message = response_json["choices"].as_array()
            .and_then(|choices| choices.first())
            .ok_or_else(|| AgentError::LlmError("No assistant response".to_string()))?;
        
        // 提取工具调用
        let tool_calls = assistant_message["message"]["tool_calls"].as_array().map(|calls| {
            calls.iter().map(|call| call.clone()).collect()
        });
        
        // 构建聊天响应
        let chat_response = ChatResponse {
            content: assistant_message["message"]["content"].as_str().map(|s| s.to_string()),
            id: response_json["id"].as_str().unwrap_or("").to_string(),
            tool_calls: self.map_tool_calls(tool_calls),
        };
        
        Ok(chat_response)
    }
}