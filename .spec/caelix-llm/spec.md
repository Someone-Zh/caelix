# LLM Provider 规范

## 功能概述

LLM Provider 是 Caelix 与大语言模型 API 通信的抽象层，目前支持 OpenAI。通过 `LlmProvider` trait 定义统一接口，可扩展支持其他提供商（Anthropic、Google、本地模型等）。支持流式响应、工具调用、多模型切换。

## 核心能力

### 1. Provider 接口

**LlmProvider Trait**:
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    fn config(&self) -> Arc<ProviderConfig>;
    
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError>;
}
```

**关键方法**:
- `config()`: 返回 Provider 配置
- `chat_stream()`: 流式聊天，支持工具调用

### 2. 消息格式

**ChatMessage**:
```rust
pub struct ChatMessage {
    pub role: String,              // "system", "user", "assistant", "tool"
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self { /* ... */ }
    pub fn user(content: impl Into<String>) -> Self { /* ... */ }
    pub fn assistant(content: impl Into<String>) -> Self { /* ... */ }
    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self { /* ... */ }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self { /* ... */ }
}
```

**角色说明**:
- **system**: 系统提示词，定义 Agent 行为
- **user**: 用户输入
- **assistant**: AI 助手回复
- **tool**: 工具执行结果

### 3. 流式响应

**ChatResponseChunk**:
```rust
pub struct ChatResponseChunk {
    pub reasoning_content: Option<String>,  // 推理过程（可选）
    pub content: Option<String>,            // 回答内容
    pub id: String,                         // 响应 ID
    pub tool_calls: Option<Vec<ToolCall>>,  // 工具调用
    pub finish_reason: Option<String>,      // 结束原因
}
```

**流式处理流程**:
```
LLM API (SSE)
      ↓
接收 Chunk
      ↓
解析 JSON
      ↓
转换为 ChatResponseChunk
      ↓
通过 Stream 返回
      ↓
Agent 消费 Stream
```

### 4. 工具调用

**ToolCall 结构**:
```rust
pub struct ToolCall {
    pub id: String,
    pub index: u32,
    pub name: String,
    pub arguments: serde_json::Value,
}
```

**工具调用流程**:
```
Agent 构建消息（包含 tools 定义）
      ↓
发送给 LLM
      ↓
LLM 返回 tool_calls
      ↓
Agent 解析并执行工具
      ↓
将工具结果作为 tool 消息返回
      ↓
LLM 基于工具结果继续生成
```

## OpenAI Provider 实现

### 1. 配置结构

**ProviderConfig**:
```yaml
name: openai
llm_type: OpenAI
api_key: ${OPENAI_API_KEY}
base_url: https://api.openai.com/v1
max_tokens: 4096
temperature: 0.7
models:
  gpt-4: gpt-4
  gpt-3.5-turbo: gpt-3.5-turbo
options:
  timeout: 30
  max_retries: 3
```

### 2. 实现细节

**OpenAiProvider 结构**:
```rust
#[derive(Debug)]
pub struct OpenAiProvider {
    config: Arc<ProviderConfig>,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(
                config.options.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30)
            ))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            config: Arc::new(config),
            client,
        }
    }
}
```

**chat_stream 实现**:
```rust
#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn config(&self) -> Arc<ProviderConfig> {
        self.config.clone()
    }
    
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        // 1. 构建请求体
        let request_body = self.build_request(messages, tools, config)?;
        
        // 2. 发送 HTTP 请求
        let response = self.client
            .post(format!("{}/chat/completions", self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1")))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::ProviderError(format!("HTTP request failed: {}", e)))?;
        
        // 3. 检查响应状态
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::ProviderError(format!("API error: {}", error_text)));
        }
        
        // 4. 获取 SSE 流
        let stream = response.bytes_stream();
        
        // 5. 解析 SSE 数据
        let parsed_stream = stream
            .map_err(|e| AgentError::ProviderError(format!("Stream error: {}", e)))
            .and_then(|bytes| async move {
                // 解析 SSE 格式
                let text = String::from_utf8(bytes.to_vec())
                    .map_err(|e| AgentError::ProviderError(format!("UTF-8 error: {}", e)))?;
                
                // 分割多个 SSE 事件
                let events: Vec<&str> = text.split("\n\n").collect();
                
                // 转换为 futures stream
                let chunks = events.into_iter().filter_map(|event| {
                    if event.starts_with("data: ") {
                        let data = &event[6..];
                        if data == "[DONE]" {
                            None
                        } else {
                            Some(serde_json::from_str::<OpenAIChunk>(data)
                                .map_err(|e| AgentError::ProviderError(format!("JSON parse error: {}", e))))
                        }
                    } else {
                        None
                    }
                });
                
                futures::stream::iter(chunks)
            })
            .flatten()
            .map(|openai_chunk| {
                // 转换为统一的 ChatResponseChunk
                Ok(self.convert_chunk(openai_chunk?))
            });
        
        Ok(Box::pin(parsed_stream))
    }
}
```

### 3. 请求构建

**构建 OpenAI API 请求**:
```rust
fn build_request(
    &self,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    config: &LlmConfig,
) -> Result<serde_json::Value, AgentError> {
    let mut request = serde_json::json!({
        "model": config.model_name,
        "messages": messages.iter().map(|msg| {
            let mut obj = serde_json::json!({
                "role": msg.role,
                "content": msg.content
            });
            
            if let Some(tool_calls) = &msg.tool_calls {
                obj["tool_calls"] = serde_json::to_value(
                    tool_calls.iter().map(|tc| tc.to_api_format()).collect::<Vec<_>>()
                ).unwrap();
            }
            
            if let Some(tool_call_id) = &msg.tool_call_id {
                obj["tool_call_id"] = serde_json::Value::String(tool_call_id.clone());
            }
            
            obj
        }).collect::<Vec<_>>(),
        "stream": true,
    });
    
    // 添加工具定义
    if !tools.is_empty() {
        request["tools"] = serde_json::to_value(
            tools.iter().map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters_schema
                    }
                })
            }).collect::<Vec<_>>()
        ).unwrap();
    }
    
    // 添加可选参数
    if let Some(max_tokens) = self.config.max_tokens {
        request["max_tokens"] = serde_json::Value::Number(max_tokens.into());
    }
    
    if let Some(temperature) = self.config.temperature {
        request["temperature"] = serde_json::Value::from(temperature);
    }
    
    Ok(request)
}
```

### 4. 响应转换

**OpenAI Chunk 结构**:
```rust
#[derive(Debug, Deserialize)]
struct OpenAIChunk {
    id: String,
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}
```

**转换为统一格式**:
```rust
fn convert_chunk(&self, openai_chunk: OpenAIChunk) -> ChatResponseChunk {
    let choice = &openai_chunk.choices[0];
    
    ChatResponseChunk {
        id: openai_chunk.id.clone(),
        content: choice.delta.content.clone(),
        reasoning_content: choice.delta.reasoning_content.clone(),
        tool_calls: choice.delta.tool_calls.as_ref().map(|tool_calls| {
            tool_calls.iter().map(|tc| ToolCall {
                id: tc.id.clone(),
                index: tc.index,
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
            }).collect()
        }),
        finish_reason: choice.finish_reason.clone(),
    }
}
```

## 扩展指南

### 添加新 Provider

**以 Anthropic 为例**:

1. **定义配置**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: Option<String>,
    pub models: HashMap<String, String>,
}
```

2. **实现 Provider**
```rust
#[derive(Debug)]
pub struct AnthropicProvider {
    config: Arc<ProviderConfig>,
    client: reqwest::Client,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn config(&self) -> Arc<ProviderConfig> {
        self.config.clone()
    }
    
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        // 实现 Anthropic API 调用逻辑
        // ...
    }
}
```

3. **注册 Provider**
```rust
// caelix-config/src/provider_loader.rs
match config.llm_type {
    LlmType::OpenAI => Arc::new(OpenAiProvider::new(config)),
    LlmType::Anthropic => Arc::new(AnthropicProvider::new(config)),
    // ...
}
```

4. **添加配置文件**
```yaml
# ~/.caelix/providers/anthropic.yaml
name: anthropic
llm_type: Anthropic
api_key: ${ANTHROPIC_API_KEY}
base_url: https://api.anthropic.com/v1
models:
  claude-3-opus: claude-3-opus-20240229
  claude-3-sonnet: claude-3-sonnet-20240229
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `AuthenticationError` | API Key 无效 | 提示用户检查配置 |
| `RateLimitError` | 超过速率限制 | 指数退避重试 |
| `TimeoutError` | 请求超时 | 增加超时时间或重试 |
| `InvalidRequestError` | 请求参数错误 | 记录详细错误信息 |
| `ApiUnavailableError` | API 服务不可用 | 降级或提示用户 |

### 重试机制

```rust
const MAX_RETRIES: usize = 3;

for attempt in 1..=MAX_RETRIES {
    match self.send_request(&request_body).await {
        Ok(response) => return Ok(response),
        Err(e) if is_retryable(&e) && attempt < MAX_RETRIES => {
            warn!("Request failed (attempt {}): {:?}", attempt, e);
            let delay = Duration::from_secs(attempt as u64 * 2);
            tokio::time::sleep(delay).await;
        },
        Err(e) => {
            error!("Request failed after {} attempts: {:?}", MAX_RETRIES, e);
            return Err(e);
        }
    }
}
```

## 性能优化

### 1. 连接池

**复用 HTTP 客户端**:
```rust
// 在 Provider 初始化时创建，整个生命周期复用
let client = reqwest::Client::builder()
    .pool_max_idle_per_host(10)
    .timeout(Duration::from_secs(30))
    .build()?;
```

### 2. 流式处理

**避免缓冲整个响应**:
```rust
// 使用 bytes_stream 逐块处理
let stream = response.bytes_stream();

// 而不是
let full_response = response.text().await?;
```

### 3. 并发控制

**限制并发请求数**:
```rust
use tokio::sync::Semaphore;

static SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(10));

async fn send_request(&self, body: &Value) -> Result<Response, Error> {
    let _permit = SEMAPHORE.acquire().await?;
    // 发送请求
}
```

## 测试策略

### Mock Provider

```rust
#[derive(Debug)]
pub struct MockProvider {
    responses: Vec<ChatResponseChunk>,
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn config(&self) -> Arc<ProviderConfig> {
        // 返回测试配置
    }
    
    async fn chat_stream(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseChunk, AgentError>> + Send>>, AgentError> {
        let stream = futures::stream::iter(
            self.responses.clone().into_iter().map(Ok)
        );
        Ok(Box::pin(stream))
    }
}
```

### 单元测试

```rust
#[tokio::test]
async fn test_openai_provider() {
    let config = create_test_config();
    let provider = OpenAiProvider::new(config);
    
    let messages = vec![ChatMessage::user("Hello")];
    let stream = provider.chat_stream(&messages, &[], &LlmConfig {
        model_name: "gpt-4".to_string(),
    }).await.unwrap();
    
    // 验证流式响应
}
```

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
