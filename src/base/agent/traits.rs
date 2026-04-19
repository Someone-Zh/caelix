use crate::base::tool::traits::ToolDefinition;
use crate::base::{AgentError, LlmConfig, Tool};
use crate::base::provider::{ChatMessage, ChatResponseChunk, LlmProvider};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentOutputChunk {
    Reasoning { content: String },
    Content { content: String },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        tool_name: String,
        result: String },
    Finish { reason: String },
}

impl std::fmt::Display for AgentOutputChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentOutputChunk::Reasoning { content } => write!(f, "{}", content),
            AgentOutputChunk::Content { content } => write!(f, "{}", content),
            AgentOutputChunk::ToolCall { name, .. } => write!(f, "[工具调用: {}]", name),
            AgentOutputChunk::ToolResult { result, .. } => write!(f, "{}", result),
            AgentOutputChunk::Finish { .. } => write!(f, ""),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub system_prompt: String,
    pub tools: Vec<Arc<dyn Tool>>,
}

impl AgentSpec {
    pub fn new(
        name: String,
        system_prompt: String,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        Self { name, system_prompt, tools }
    }

    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.to_definition()).collect()
    }

    pub async fn execute(
        &self,
        user_input: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
        let messages = self.build_messages(user_input);
        self.run_loop(messages, llm_provider, config.clone()).await
    }

    fn build_messages(&self, user_input: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage::system(self.system_prompt.clone())];
        messages.extend(user_input);
        messages
    }

    async fn run_loop(
        &self,
        messages: Vec<ChatMessage>,
        llm_provider: Arc<dyn LlmProvider>,
        config: LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentOutputChunk, AgentError>> + Send>>, AgentError> {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let agent = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut current_messages = messages;
            loop {
                let tool_defs = agent.get_tool_definitions();
                let stream = match llm_provider.chat_stream(&current_messages, &tool_defs, &config).await {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                };

                let mut full_content = String::new();
                let mut s = stream;

                // ========================
                // 工具参数流式拼接（最终修复）
                // ========================
                let mut tool_call_buffer: Option<(String, String, String)> = None;

                while let Some(result) = s.next().await {
                    match result {
                        Ok(chunk) => {
                            if let Some(c) = &chunk.content {
                                full_content.push_str(c);
                            }

                            // ✅ 正确处理 Value 类型的 arguments
                            if let Some(tcs) = &chunk.tool_calls {
                                for tc in tcs {
                                    if tool_call_buffer.is_none() {
                                        tool_call_buffer = Some((
                                            tc.id.clone(),
                                            tc.name.clone(),
                                            String::new(),
                                        ));
                                    }
                                    if let Some((_, _, args)) = &mut tool_call_buffer {
                                        args.push_str(tc.arguments.as_str().unwrap_or(""));
                                    }
                                }
                            }

                            // 发送非工具调用的 chunk
                            let converted = Self::convert_chunk(chunk);
                            if let Ok(AgentOutputChunk::ToolCall { .. }) = converted {
                                continue;
                            }
                            let _ = tx.send(converted).await;
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e)).await;
                            return;
                        }
                    }
                }

                // ========================
                // 发送完整工具调用
                // ========================
                let mut final_tool_call = None;
                if let Some((id, name, args)) = tool_call_buffer {
                    let clean_args = args.trim().to_string();
                    final_tool_call = Some((id.clone(), name.clone(), clean_args.clone()));
                    
                    let _ = tx.send(Ok(AgentOutputChunk::ToolCall {
                        tool_call_id: id,
                        name: name.clone(),
                        arguments: clean_args,
                    })).await;
                }

                // 结束判断
                if final_tool_call.is_none() {
                    let _ = tx.send(Ok(AgentOutputChunk::Finish { reason: "stop".into() })).await;
                    break;
                }

                // 执行工具
                let mut tool_results = Vec::new();
                if let Some((tc_id, tc_name, tc_args)) = final_tool_call {
                    let tool = match agent.tools.iter().find(|t| t.name() == tc_name) {
                        Some(t) => t,
                        None => {
                            let msg = format!("工具不存在：{}", tc_name);
                            let _ = tx.send(Err(AgentError::ToolError(msg))).await;
                            return;
                        }
                    };
                    let clean_json_str = serde_json::from_str::<String>(&tc_args).unwrap_or_else(|_| tc_args.clone());

                    let args_json: serde_json::Value = serde_json::from_str(&clean_json_str).unwrap_or_else(|_| {
                        eprintln!("工具参数解析失败: {}", tc_args);
                        serde_json::json!({})
                    });
                    println!("=== 最终正确参数: {:?}", args_json);
                    let result = tool.execute(args_json).await;

                    let result_str = match result.error {
                        Some(err) => format!("工具执行错误：{}", err),
                        None => result.output.to_string(),
                    };

                    tool_results.push((tc_id, tc_name.clone(), result_str.clone()));

                    let _ = tx.send(Ok(AgentOutputChunk::ToolResult {
                        tool_name: tc_name,
                        result: result_str,
                    })).await;
                }

                // 更新上下文
                current_messages.push(ChatMessage::assistant(full_content));
                for (_, _, result) in tool_results {
                    current_messages.push(ChatMessage::tool(result));
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    fn convert_chunk(chunk: ChatResponseChunk) -> Result<AgentOutputChunk, AgentError> {
        if let Some(r) = chunk.reasoning_content.filter(|s| !s.is_empty()) {
            return Ok(AgentOutputChunk::Reasoning { content: r });
        }
        if let Some(c) = chunk.content.filter(|s| !s.is_empty()) {
            return Ok(AgentOutputChunk::Content { content: c });
        }
        if let Some(r) = chunk.finish_reason {
            return Ok(AgentOutputChunk::Finish { reason: r });
        }
        Ok(AgentOutputChunk::Content { content: String::new() })
    }
}