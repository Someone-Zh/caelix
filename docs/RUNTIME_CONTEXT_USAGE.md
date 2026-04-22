# RuntimeContext 使用指南

## 概述

`RuntimeContext` 是一个 Session 级别的运行时上下文，提供了对会话 ID、请求 ID、追踪 Span ID、工作目录和全局 CaelixContext 的便捷访问。

## 核心特性

- ✅ **Session 隔离**：每个 Session 有独立的上下文
- ✅ **自动追踪集成**：从 tracing span 自动提取 span_id
- ✅ **协程安全**：使用 `tokio::task_local!` 实现
- ✅ **静态访问**：通过静态方法在任何地方访问当前上下文
- ✅ **Request 级别追踪**：支持 session_id 和 request_id 双层追踪

## 快速开始

### 1. 创建 Session 上下文

```rust
use caelix::runtime::RuntimeContext;
use caelix::config::CaelixContext;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 初始化全局 CaelixContext
    let caelix_ctx = Arc::new(CaelixContext::new());
    caelix_ctx.init().await.expect("Failed to initialize");
    
    // 创建并进入 Session 上下文
    let work_dir = std::env::current_dir().unwrap();
    
    RuntimeContext::with_session(
        Some("my_session_123".to_string()), // Session ID（可选）
        work_dir,                            // 工作目录
        caelix_ctx,                          // 全局上下文
        async {
            // 在此闭包内可以访问上下文
            println!("Session: {}", RuntimeContext::session_id());
            println!("Request: {}", RuntimeContext::request_id());
            
            // 执行业务逻辑...
            my_business_logic().await;
        }
    ).await;
}
```

### 2. 在任何地方访问上下文

```rust
async fn my_business_logic() {
    // 获取完整的上下文
    let ctx = RuntimeContext::current();
    
    // 或者使用便捷方法获取单个字段
    let session_id = RuntimeContext::session_id();
    let request_id = RuntimeContext::request_id();
    let span_id = RuntimeContext::span_id();
    let work_dir = RuntimeContext::work_dir();
    let caelix_ctx = RuntimeContext::caelix_context();
    
    println!("Current session: {}", session_id);
}
```

### 3. 在嵌套作用域中使用

```rust
async fn nested_example() {
    println!("Outer - Session: {}", RuntimeContext::session_id());
    
    // 可以在同一个 Session 中创建新的 Request
    let new_request_ctx = RuntimeContext::current();
    // 注意：这里只是示例，实际应该用新的 request_id 创建新上下文
    
    some_deeply_nested_function().await;
}

async fn some_deeply_nested_function() {
    // 即使在深层嵌套中也能访问上下文
    let session_id = RuntimeContext::session_id();
    println!("Deep nested - Session: {}", session_id);
}
```

### 4. 与 Message 集成

```rust
use caelix::runtime::message::{Message, Role, MessageType, Status};

async fn create_message_example() {
    // 方式 1：手动指定 session_id 和 span_id
    let msg1 = Message::new(
        RuntimeContext::session_id(),
        RuntimeContext::span_id(),
        None,
        Role::User,
        "user".to_string(),
        MessageType::Chunk,
        "Hello".to_string(),
        Status::Running,
    );
    
    // 方式 2：自动从上下文获取（推荐）
    let msg2 = Message::from_context(
        None,  // parent_span_id
        Role::Agent,
        "assistant".to_string(),
        MessageType::Chunk,
        "Hi there!".to_string(),
        Status::Running,
    );
    
    // msg2 会自动填充当前上下文的 session_id 和 span_id
}
```

### 5. Tracing 集成

```rust
use tracing;

async fn tracing_example() {
    // 创建带 session_id 的 span
    let session_id = RuntimeContext::session_id();
    let span = tracing::info_span!("agent_execution", session_id = %session_id);
    
    // 在 span 内执行，span_id 会自动提取
    span.in_scope(|| {
        // 这里的代码会自动使用当前的 tracing span id
        let current_span_id = RuntimeContext::span_id();
        println!("Current span: {}", current_span_id);
    });
}
```

## API 参考

### RuntimeContext 结构

```rust
pub struct RuntimeContext {
    session_id: String,      // Session ID
    request_id: String,      // Request ID
    span_id: String,         // Span ID（从 tracing 提取）
    work_dir: PathBuf,       // 工作目录
    caelix_context: Arc<CaelixContext>,  // 全局上下文引用
}
```

### 静态方法

| 方法 | 返回类型 | 说明 |
|------|---------|------|
| `current()` | `RuntimeContext` | 获取当前完整的上下文实例 |
| `session_id()` | `String` | 获取当前 Session ID |
| `request_id()` | `String` | 获取当前 Request ID |
| `span_id()` | `String` | 获取当前 Span ID |
| `work_dir()` | `PathBuf` | 获取当前工作目录 |
| `caelix_context()` | `Arc<CaelixContext>` | 获取全局 CaelixContext |

### 实例方法

| 方法 | 返回类型 | 说明 |
|------|---------|------|
| `get_session_id()` | `&str` | 获取 Session ID 引用 |
| `get_request_id()` | `&str` | 获取 Request ID 引用 |
| `get_span_id()` | `&str` | 获取 Span ID 引用 |
| `get_work_dir()` | `&PathBuf` | 获取工作目录引用 |
| `get_caelix_context()` | `&Arc<CaelixContext>` | 获取全局上下文引用 |

### 作用域管理

```rust
// 异步作用域
RuntimeContext::scope(context, async {
    // 业务逻辑
}).await;

// 同步作用域
RuntimeContext::scope_sync(context, || {
    // 业务逻辑
});

// 便捷 Session 创建
RuntimeContext::with_session(session_id, work_dir, caelix_ctx, async {
    // 业务逻辑
}).await;
```

## 最佳实践

### 1. Session 生命周期管理

```rust
// ✅ 推荐：在请求入口处创建 Session
async fn handle_request(request: HttpRequest) {
    let session_id = request.headers.get("X-Session-ID")
        .cloned()
        .unwrap_or_else(|| generate_session_id());
    
    RuntimeContext::with_session(
        Some(session_id),
        get_work_dir(),
        get_caelix_context(),
        async move {
            process_request(request).await;
        }
    ).await;
}
```

### 2. 错误处理

```rust
// ⚠️ 注意：在不存在的上下文中调用会 panic
// 确保在 RuntimeContext::scope 或 with_session 的作用域内调用

async fn safe_access() -> Option<String> {
    // 如果需要安全检查，可以使用 try_current（未来可能添加）
    // 目前建议通过架构设计保证上下文存在
    Some(RuntimeContext::session_id())
}
```

### 3. 并发场景

```rust
// ✅ 每个任务继承父任务的上下文
async fn concurrent_example() {
    let session_id = RuntimeContext::session_id();
    
    let handles: Vec<_> = (0..5)
        .map(|i| {
            tokio::spawn(async move {
                // 子任务会自动继承父任务的上下文
                println!("Task {} in session {}", i, RuntimeContext::session_id());
            })
        })
        .collect();
    
    for handle in handles {
        handle.await.unwrap();
    }
}
```

## 架构说明

```
┌──────────────────────────────────────────┐
│  CaelixContext (全局单例)                 │
│  ├─ agent_manager                         │
│  ├─ tool_manager                          │
│  └─ llm_provider_manager                  │
└──────────────────────────────────────────┘
                    ↓ Arc 引用
┌──────────────────────────────────────────┐
│  RuntimeContext (Session 级别)            │
│  ├─ session_id: String                    │ ← Session 独立
│  ├─ request_id: String                    │ ← Request 独立
│  ├─ span_id: String                       │ ← 从 tracing 提取
│  ├─ work_dir: PathBuf                     │ ← Session 独立
│  └─ caelix_context: Arc<CaelixContext>   │ ← 共享引用
└──────────────────────────────────────────┘
         ↓ tokio::task_local! 存储
    每个 async 任务可访问当前 Session 的上下文
```

## 常见问题

### Q: 如何在 HTTP 服务中使用？

A: 在中间件中提取或创建 session_id，然后为每个请求创建 RuntimeContext：

```rust
async fn middleware(request: Request, next: Next) -> Response {
    let session_id = extract_or_create_session(&request);
    let work_dir = get_work_dir_for_session(&session_id);
    
    RuntimeContext::with_session(
        Some(session_id),
        work_dir,
        get_global_context(),
        async move {
            next.run(request).await
        }
    ).await
}
```

### Q: span_id 是如何提取的？

A: 自动从 `tracing::Span::current()` 提取。如果没有活跃的 span，会生成一个新的 UUID。

### Q: 可以动态修改工作目录吗？

A: 不可以。工作目录在 Session 创建时设定，之后只读。这是为了保证线程安全和一致性。

### Q: 如何在测试中使用？

A: 使用 `RuntimeContext::scope` 或 `with_session` 包装测试代码：

```rust
#[tokio::test]
async fn test_with_context() {
    let caelix_ctx = Arc::new(CaelixContext::new());
    let work_dir = std::env::current_dir().unwrap();
    
    RuntimeContext::with_session(
        Some("test_session".to_string()),
        work_dir,
        caelix_ctx,
        async {
            // 测试逻辑
            assert_eq!(RuntimeContext::session_id(), "test_session");
        }
    ).await;
}
```

## 更多信息

- 查看 `src/runtime/context/runtime_context.rs` 了解完整实现
- 运行 `cargo test runtime::context` 执行单元测试
- 参考 `src/main.rs` 查看完整的使用示例
