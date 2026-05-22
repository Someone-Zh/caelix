# HTTP API 服务规范

## 功能概述

HTTP API 服务是 Caelix 的 RESTful API 接口层，将 CaelixApi 暴露为 HTTP 端点，支持远程调用。使用 axum 框架构建，支持 CORS、错误处理、流式响应，适合集成到其他系统或构建 Web 应用。

## 核心能力

### 1. API 端点

**基础端点**:

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/chat` | 同步聊天（等待完整响应） |
| POST | `/api/chat/stream` | 流式聊天（SSE） |
| POST | `/api/chat/async` | 异步触发聊天（后台执行） |
| GET | `/api/sessions` | 获取会话列表 |
| GET | `/api/sessions/{id}/messages` | 获取会话消息历史 |
| GET | `/api/sessions/{id}/notifications` | 获取会话通知 |
| POST | `/api/sessions` | 创建新会话 |
| GET | `/api/agents` | 获取 Agent 列表 |
| GET | `/api/providers` | 获取 Provider 列表 |
| GET | `/api/providers/{name}/models` | 获取 Provider 的模型列表 |
| GET | `/api/tasks` | 获取任务列表 |
| POST | `/api/sessions/{id}/provider` | 设置会话的 Provider |
| POST | `/api/sessions/{id}/model` | 设置会话的 Model |

### 2. 请求/响应格式

**聊天请求** (`POST /api/chat`):
```json
{
  "session_id": "sess_abc123",
  "agent_name": "planner_agent",
  "message": "帮我分析项目架构",
  "provider": "openai",
  "model": "gpt-4"
}
```

**聊天响应** (同步):
```json
{
  "content": "这个项目采用了分层架构...",
  "tool_calls": [],
  "finish_reason": "stop"
}
```

**流式响应** (`POST /api/chat/stream`):
```
event: chunk
data: {"type":"content","content":"这个"}

event: chunk
data: {"type":"content","content":"项目"}

event: chunk
data: {"type":"finish","reason":"stop"}
```

**错误响应**:
```json
{
  "error": {
    "code": "AGENT_NOT_FOUND",
    "message": "Agent 'invalid_agent' not found",
    "details": null
  }
}
```

### 3. Server-Sent Events (SSE)

**流式聊天实现**:
```rust
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;

pub async fn chat_stream(
    State(api): State<Arc<CaelixApi>>,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let stream = api.chat_stream(request).await?;
    
    let sse_stream = stream.map(|chunk_result| {
        match chunk_result {
            Ok(chunk) => {
                let json = serde_json::to_string(&chunk).unwrap();
                Ok(Event::default().event("chunk").data(json))
            },
            Err(e) => {
                let error_json = serde_json::to_string(&ApiErrorResponse::from(e)).unwrap();
                Ok(Event::default().event("error").data(error_json))
            }
        }
    });
    
    Ok(Sse::new(sse_stream))
}
```

**前端订阅示例**:
```javascript
const eventSource = new EventSource('/api/chat/stream', {
  headers: {
    'Content-Type': 'application/json',
  },
});

eventSource.addEventListener('chunk', (event) => {
  const chunk = JSON.parse(event.data);
  console.log(chunk.content);
});

eventSource.addEventListener('error', (event) => {
  console.error('Error:', event.data);
  eventSource.close();
});
```

### 4. CORS 支持

**配置 CORS**:
```rust
use tower_http::cors::{CorsLayer, Any};

fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
```

**使用**:
```rust
let app = Router::new()
    .nest("/api", api_routes())
    .layer(create_cors_layer());
```

### 5. 错误处理

**统一错误响应**:
```rust
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl From<ApiError> for ApiErrorResponse {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::AgentNotFound(name) => ApiErrorResponse {
                error: ErrorDetail {
                    code: "AGENT_NOT_FOUND".to_string(),
                    message: format!("Agent '{}' not found", name),
                    details: None,
                }
            },
            ApiError::ProviderError(msg) => ApiErrorResponse {
                error: ErrorDetail {
                    code: "PROVIDER_ERROR".to_string(),
                    message: msg,
                    details: None,
                }
            },
            _ => ApiErrorResponse {
                error: ErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: "Internal server error".to_string(),
                    details: Some(serde_json::json!({ "error": format!("{:?}", err) })),
                }
            }
        }
    }
}
```

**错误处理中间件**:
```rust
async fn handle_error(
    err: Result<Response, ApiError>,
) -> Result<Response, ApiError> {
    match err {
        Ok(response) => Ok(response),
        Err(e) => {
            let error_response = ApiErrorResponse::from(e);
            let status = match &error_response.error.code.as_str() {
                "AGENT_NOT_FOUND" => StatusCode::NOT_FOUND,
                "VALIDATION_ERROR" => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            
            let body = serde_json::to_string(&error_response).unwrap();
            Ok(Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap())
        }
    }
}
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **Server** | `caelix-http/src/server.rs` | HTTP 服务器启动和配置 |
| **Handlers** | `caelix-http/src/handlers.rs` | HTTP 请求处理器 |

### Server 实现

```rust
use axum::{Router, routing::post, extract::State};
use tokio::net::TcpListener;

pub async fn start_http_server(
    api: Arc<CaelixApi>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // 创建路由
    let app = Router::new()
        .route("/api/chat", post(chat_sync))
        .route("/api/chat/stream", post(chat_stream))
        .route("/api/chat/async", post(chat_async))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id/messages", get(get_session_messages))
        .route("/api/agents", get(list_agents))
        .route("/api/providers", get(get_providers))
        .route("/api/tasks", get(list_tasks))
        .with_state(api)
        .layer(create_cors_layer())
        .layer(TraceLayer::new_for_http());
    
    // 启动服务器
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    println!("🌐 HTTP Server listening on {}", addr);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
```

### Handler 实现

**同步聊天**:
```rust
pub async fn chat_sync(
    State(api): State<Arc<CaelixApi>>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    // 调用 API
    let mut stream = api.chat_stream(request).await?;
    
    // 收集所有 chunk
    let mut content = String::new();
    while let Some(chunk_result) = stream.next().await {
        match chunk_result? {
            AgentOutputChunk::Content { content: c } => {
                content.push_str(&c);
            },
            _ => {}
        }
    }
    
    Ok(Json(ChatResponse {
        content,
        tool_calls: vec![],
        finish_reason: "stop".to_string(),
    }))
}
```

**获取会话列表**:
```rust
pub async fn list_sessions(
    State(api): State<Arc<CaelixApi>>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let sessions = api.list_sessions().await?;
    Ok(Json(sessions))
}
```

**创建会话**:
```rust
pub async fn create_session(
    State(api): State<Arc<CaelixApi>>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    let session_id = api.create_session().await;
    Ok(Json(CreateSessionResponse { session_id }))
}
```

## 安全规范

### 1. 认证授权（待实现）

**API Key 认证**:
```rust
async fn authenticate(
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let api_key = headers.get("X-API-Key")
        .ok_or_else(|| ApiError::Unauthorized)?
        .to_str()
        .map_err(|_| ApiError::Unauthorized)?;
    
    if !validate_api_key(api_key) {
        return Err(ApiError::Unauthorized);
    }
    
    Ok(())
}
```

**中间件集成**:
```rust
let app = Router::new()
    .route("/api/chat", post(chat_sync))
    .layer(middleware::from_fn(authenticate));
```

### 2. 速率限制（待实现）

**使用 tower-limit**:
```rust
use tower_limit::RateLimitLayer;

let app = Router::new()
    .route("/api/chat", post(chat_sync))
    .layer(RateLimitLayer::new(100, Duration::from_secs(60)));
```

### 3. 输入验证

**请求体大小限制**:
```rust
use axum::extract::DefaultBodyLimit;

let app = Router::new()
    .route("/api/chat", post(chat_sync))
    .layer(DefaultBodyLimit::max(1024 * 1024)); // 1MB
```

**参数验证**:
```rust
fn validate_chat_request(request: &ChatRequest) -> Result<(), ApiError> {
    if request.message.is_empty() {
        return Err(ApiError::ValidationError("Message cannot be empty".to_string()));
    }
    
    if request.message.len() > 10000 {
        return Err(ApiError::ValidationError("Message too long".to_string()));
    }
    
    Ok(())
}
```

## 性能优化

### 1. 连接池

**复用 TCP 连接**:
```rust
// axum 默认使用 connection pooling
// 可通过 keep-alive 优化
let listener = TcpListener::bind(&addr).await?;
listener.set_ttl(60).ok();
```

### 2. 异步处理

**非阻塞 I/O**:
```rust
// 所有 handler 都是 async 函数
pub async fn chat_sync(...) -> Result<...> {
    // 异步调用 API
    let stream = api.chat_stream(request).await?;
    // ...
}
```

### 3. 缓存

**响应缓存**（可选）:
```rust
use tower_http::services::ServeDir;

// 静态资源缓存
let app = Router::new()
    .nest_service("/static", ServeDir::new("static"))
    .layer(SetResponseHeaderLayer::overriding(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    ));
```

## 扩展指南

### 添加新端点

**步骤**:

1. **定义 Handler**
```rust
pub async fn custom_endpoint(
    State(api): State<Arc<CaelixApi>>,
    Path(id): Path<String>,
) -> Result<Json<CustomResponse>, ApiError> {
    // 实现逻辑
    Ok(Json(CustomResponse { data: "..." }))
}
```

2. **注册路由**
```rust
let app = Router::new()
    .route("/api/custom/:id", get(custom_endpoint))
    .with_state(api);
```

3. **添加文档**
```rust
/// 自定义端点描述
/// 
/// # Parameters
/// - `id`: 资源 ID
/// 
/// # Returns
/// 返回自定义数据
pub async fn custom_endpoint(...) { ... }
```

### WebSocket 支持（未来扩展）

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(handle_websocket)
}

async fn handle_websocket(socket: WebSocket) {
    // WebSocket 逻辑
}
```

## 测试策略

### 单元测试

```rust
#[tokio::test]
async fn test_chat_sync_handler() {
    let api = create_mock_api();
    let request = ChatRequest {
        message: "Hello".to_string(),
        ..Default::default()
    };
    
    let response = chat_sync(State(api), Json(request)).await;
    assert!(response.is_ok());
}
```

### 集成测试

```rust
use axum::http::StatusCode;
use axum::body::Body;
use tower::ServiceExt;

#[tokio::test]
async fn test_chat_endpoint() {
    let api = create_test_api();
    let app = create_router(api);
    
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&ChatRequest::default()).unwrap()))
                .unwrap()
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}
```

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
