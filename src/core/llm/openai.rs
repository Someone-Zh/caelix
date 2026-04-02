use super::*;
use async_openai::{Client, types::{ChatCompletionRequestMessage, CreateChatCompletionRequestArgs, Role, ChatCompletionChunk, ChatCompletionResponse, ChatCompletionMessageToolCall}};

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    client: Client,
    models: std::collections::HashMap<String, String>,
}

impl OpenAIProvider {
    pub fn new(api_key: String, base_url: Option<String>, models: std::collections::HashMap<String, String>) -> Self {
        let mut client_builder = Client::builder().api_key(api_key);
        
        if let Some(base_url) = base_url {
            client_builder = client_builder.base_url(base_url);
        }
        
        Self {
            client: client_builder.build().unwrap(),
            models,
        }
    }
    
    fn map_messages(&self, messages: Vec<Message>) -> Vec<ChatCompletionRequestMessage> {
        messages.into_iter().map(|msg| {
            let role = match msg.role {
                MessageRole::User => Role::User,
                MessageRole::Assistant => Role::Assistant,
                MessageRole::System => Role::System,
            };
            ChatCompletionRequestMessage {
                role,
                content: Some(msg.content),
                name: None,
                function_call: None,
                tool_calls: None,
                tool_call_id: None,
            }
        }).collect()
    }
    
    fn map_tool_calls(&self, tool_calls: Option<Vec<ChatCompletionMessageToolCall>>) -> Option<Vec<ToolCall>> {
        tool_calls.map(|calls| {
            calls.into_iter().map(|call| {
                ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::Value::Null),
                }
            }).collect()
        })
    }
    
    fn get_model(&self, model_name: &str) -> String {
        self.models.get(model_name).cloned().unwrap_or(model_name.to_string())
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        let openai_messages = self.map_messages(messages);
        let model = self.get_model(&config.model_name);
        
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(openai_messages)
            .temperature(config.temperature)
            .max_tokens(config.max_tokens)
            .stream(true)
            .build()
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        let stream = self.client.chat().create_stream(request)
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // 处理流式响应
        tokio::spawn(async move {
            let mut stream = stream;
            let mut collected_tool_calls: Vec<ToolCall> = Vec::new();
            let mut has_tool_calls = false;
            let mut response_id = String::new();
            
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(response) => {
                        response_id = response.id.clone();
                        
                        if let Some(choice) = response.choices.first() {
                            // 检查是否有工具调用
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                has_tool_calls = true;
                                // 收集工具调用信息
                                for call in tool_calls {
                                    collected_tool_calls.push(ToolCall {
                                        id: call.id.clone(),
                                        name: call.function.name.clone(),
                                        arguments: serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::Value::Null),
                                    });
                                }
                            }
                            
                            // 检查是否有内容
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    // 文字内容，每个chunk都返回
                                    let chunk = ChatResponseChunk {
                                        content: Some(content.clone()),
                                        id: response.id.clone(),
                                        tool_calls: None,
                                        finish_reason: choice.finish_reason.clone(),
                                    };
                                    
                                    if tx.send(Ok(chunk)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            
                            // 检查是否完成
                            if let Some(finish_reason) = &choice.finish_reason {
                                // 如果有工具调用，在完成时返回完整的工具调用信息
                                if has_tool_calls && !collected_tool_calls.is_empty() {
                                    let chunk = ChatResponseChunk {
                                        content: None,
                                        id: response.id.clone(),
                                        tool_calls: Some(collected_tool_calls.clone()),
                                        finish_reason: Some(finish_reason.clone()),
                                    };
                                    
                                    if tx.send(Ok(chunk)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(AgentError::LlmError(e.to_string()))).await;
                        break;
                    }
                }
            }
        });
        
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    async fn chat(
        &self,
        messages: Vec<Message>,
        config: LlmConfig,
    ) -> Result<ChatResponse, AgentError> {
        let openai_messages = self.map_messages(messages);
        let model = self.get_model(&config.model_name);
        
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(openai_messages)
            .temperature(config.temperature)
            .max_tokens(config.max_tokens)
            .stream(false)
            .build()
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        let response = self.client.chat().create(request)
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;
        
        let assistant_message = response.choices
            .into_iter()
            .find(|choice| choice.message.role == Role::Assistant)
            .ok_or_else(|| AgentError::LlmError("No assistant response".to_string()))?;
        
        let chat_response = ChatResponse {
            content: assistant_message.message.content,
            id: response.id,
            tool_calls: self.map_tool_calls(assistant_message.message.tool_calls),
        };
        
        Ok(chat_response)
    }
}