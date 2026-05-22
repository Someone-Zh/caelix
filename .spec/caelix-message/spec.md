# 消息总线系统规范

## 功能概述

消息总线系统是 Caelix 的核心通信基础设施，提供发布订阅模式的消息传递、会话管理、消息持久化和实时流式广播能力。支持多种消息类型（Agent、Notification、Task），确保消息的可靠传递和持久化存储。

## 核心能力

### 1. 消息类型

**AgentMessage**: Agent 执行过程中产生的消息
```rust
pub struct AgentMessage {
    pub id: String,              // 消息唯一 ID
    pub session_id: String,      // 会话 ID
    pub request_id: String,      // 请求 ID
    pub span_id: String,         // Span ID（追踪）
    pub r#type: AgentMessageType, // 消息类型
    pub content: String,         // 消息内容
    pub timestamp: DateTime<Utc>, // 时间戳
    pub metadata: HashMap<String, String>, // 元数据
}

pub enum AgentMessageType {
    Msg,      // 完整消息（需持久化）
    Chunk,    // 流式分片（暂存后批量持久化）
    Start,    // Agent 开始
    End,      // Agent 结束
    Error,    // 错误消息
}
```

**NotificationMessage**: 系统通知消息
```rust
pub struct NotificationMessage {
    pub id: String,
    pub session_id: String,
    pub r#type: NotificationType,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub enum NotificationType {
    TaskCreated,    // 任务创建
    TaskCompleted,  // 任务完成
    TaskFailed,     // 任务失败
    SystemInfo,     // 系统信息
    Warning,        // 警告
}
```

**TaskMessage**: 任务相关消息
```rust
pub struct TaskMessage {
    pub id: String,
    pub task_id: String,
    pub session_id: String,
    pub r#type: TaskMessageType,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub enum TaskMessageType {
    TaskStart,
    TaskProgress,
    TaskComplete,
    TaskError,
}
```

### 2. 消息总线架构

```
Publisher (Agent/Tool/Task)
       ↓
 MessageBus::publish()
       ↓
  ┌────┴────┐
  ↓         ↓
Subscriber1 Subscriber2
(SessionMgr) (Logger)
  ↓
FileStorage (持久化)
```

**核心组件**:
- **MessageBus**: 消息总线，实现发布订阅模式
- **SessionManager**: 会话管理器，订阅消息并持久化
- **FileStorage**: 文件存储，实现消息持久化

### 3. 发布订阅机制

**发布消息**:
```rust
message_bus.publish(AgentMessage {
    session_id: "session_123".to_string(),
    r#type: AgentMessageType::Msg,
    content: "Hello".to_string(),
    ..Default::default()
}).await?;
```

**订阅消息**:
```rust
let mut subscriber = message_bus.subscribe().await?;
while let Some(message) = subscriber.recv().await {
    println!("Received: {:?}", message);
}
```

**特性**:
- 支持多订阅者
- 异步非阻塞
- 消息广播到所有订阅者
- 订阅者可随时取消订阅

### 4. 会话管理

**SessionManager 职责**:
1. 创建和管理会话
2. 订阅消息总线
3. 持久化消息到文件系统
4. 提供消息查询接口
5. 管理会话生命周期

**会话结构**:
```
sessions/
├── {session_id}/
│   ├── messages.jsonl      # Agent 消息历史
│   ├── notifications.jsonl # 通知消息
│   └── tasks.jsonl         # 任务消息
```

**会话操作**:
```rust
// 创建会话
let session_id = session_manager.create_session().await?;

// 获取会话消息
let messages = session_manager.get_session_messages(&session_id).await?;

// 获取会话列表
let sessions = session_manager.list_sessions().await?;

// 删除会话
session_manager.delete_session(&session_id).await?;
```

### 5. 消息持久化

**FileStorage 实现**:
```rust
pub struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    pub async fn append_agent_message(&self, message: &AgentMessage) -> Result<(), IoError> {
        let file_path = self.base_path
            .join(&message.session_id)
            .join("messages.jsonl");
        
        let line = serde_json::to_string(message)? + "\n";
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?
            .write_all(line.as_bytes())
            .await?;
        
        Ok(())
    }
}
```

**持久化策略**:
- **Msg 类型**: 立即持久化
- **Chunk 类型**: 暂存到内存缓冲区，定期 Flush
- **Flush 时机**: 
  - 会话结束
  - 收到 Ctrl+C 信号
  - 缓冲区达到阈值
  - 定时刷新（每 5 秒）

**JSONL 格式**:
```json
{"id":"msg_001","session_id":"sess_123","type":"msg","content":"Hello","timestamp":"2026-05-22T10:00:00Z"}
{"id":"msg_002","session_id":"sess_123","type":"chunk","content":"World","timestamp":"2026-05-22T10:00:01Z"}
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **MessageBus** | `caelix-message/src/bus.rs` | 消息总线实现 |
| **SessionManager** | `caelix-message/src/manager.rs` | 会话管理器 |
| **FileStorage** | `caelix-message/src/storage.rs` | 文件存储实现 |

### MessageBus 实现

```rust
pub struct MessageBus {
    subscribers: Arc<RwLock<Vec<mpsc::Sender<AgentMessage>>>>,
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub async fn publish(&self, message: AgentMessage) -> Result<(), ApiError> {
        let subs = self.subscribers.read().await;
        for tx in subs.iter() {
            tx.send(message.clone()).await.ok();
        }
        Ok(())
    }
    
    pub async fn subscribe(&self) -> Result<mpsc::Receiver<AgentMessage>, ApiError> {
        let (tx, rx) = mpsc::channel(100);
        self.subscribers.write().await.push(tx);
        Ok(rx)
    }
}
```

### SessionManager 实现

```rust
pub struct SessionManager {
    storage: Arc<FileStorage>,
    message_bus: Arc<MessageBus>,
    agent_buffers: Arc<RwLock<HashMap<(String, String, String), Vec<AgentMessage>>>>,
}

impl SessionManager {
    pub async fn on_message(&self, message: AgentMessage) {
        match message.r#type {
            AgentMessageType::Msg => {
                // 立即持久化
                self.storage.append_agent_message(&message).await.ok();
            },
            AgentMessageType::Chunk => {
                // 暂存到缓冲区
                let key = (
                    message.session_id.clone(),
                    message.request_id.clone(),
                    message.span_id.clone(),
                );
                self.agent_buffers.write().await
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(message);
            },
            _ => {}
        }
    }
    
    pub async fn flush_buffers(&self) {
        let buffers = self.agent_buffers.read().await;
        for (_key, messages) in buffers.iter() {
            for msg in messages {
                self.storage.append_agent_message(msg).await.ok();
            }
        }
    }
}
```

### 消息流转示例

```rust
// 1. Agent 产生消息
let chunk = AgentOutputChunk::Content { content: "Hello".to_string() };

// 2. 转换为 AgentMessage
let message = AgentMessage {
    session_id: context.session_id().to_string(),
    request_id: context.request_id().to_string(),
    span_id: context.span_id().to_string(),
    r#type: AgentMessageType::Chunk,
    content: chunk.to_string(),
    timestamp: Utc::now(),
    ..Default::default()
};

// 3. 发布到消息总线
message_bus.publish(message.clone()).await?;

// 4. SessionManager 接收并处理
session_manager.on_message(message).await;

// 5. 前端订阅接收
let mut stream = message_bus.subscribe().await?;
while let Some(msg) = stream.recv().await {
    println!("{}", msg.content);
}
```

## 会话隔离

### 隔离机制

**Session ID 生成**:
```rust
pub fn generate_session_id() -> String {
    format!("sess_{}", SnowflakeIdGenerator::next_id())
}
```

**隔离策略**:
1. **存储隔离**: 每个会话独立目录
2. **消息过滤**: 订阅者只接收指定 session_id 的消息
3. **上下文绑定**: RuntimeContext 携带 session_id

**示例**:
```rust
// 创建会话时绑定 session_id
let context = RuntimeContext::new(session_id.clone());

// 所有消息自动携带 session_id
message.session_id = context.session_id().to_string();

// 查询时按 session_id 过滤
let messages = storage.get_messages_by_session(&session_id).await?;
```

## 性能优化

### 1. 批量持久化

**Chunk 消息缓冲**:
```rust
// 缓冲区大小限制
const MAX_BUFFER_SIZE: usize = 1000;

if buffer.len() >= MAX_BUFFER_SIZE {
    self.flush_buffers().await;
}
```

**定时刷新**:
```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        session_manager.flush_buffers().await;
    }
});
```

### 2. 异步 I/O

**使用 tokio 异步文件操作**:
```rust
tokio::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&file_path)
    .await?
    .write_all(line.as_bytes())
    .await?;
```

### 3. 内存管理

**限制缓冲区大小**:
```rust
if agent_buffers.len() > MAX_SESSIONS {
    // LRU 淘汰最旧的会话缓冲区
    evict_oldest_buffer();
}
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `IoError` | 文件写入失败 | 重试或记录日志 |
| `SerializeError` | JSON 序列化失败 | 跳过该消息 |
| `ChannelClosed` | 订阅者断开 | 移除订阅者 |
| `SessionNotFound` | 会话不存在 | 创建新会话 |

### 容错策略

```rust
// 持久化失败不影响主流程
if let Err(e) = storage.append_agent_message(&message).await {
    error!("Failed to persist message: {:?}", e);
    // 可选：写入备用日志
}
```

## 扩展指南

### 添加新消息类型

1. **定义消息结构**
```rust
pub struct CustomMessage {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}
```

2. **在 MessageBus 中支持**
```rust
message_bus.publish_custom(message).await?;
```

3. **添加订阅者**
```rust
let mut sub = message_bus.subscribe_custom().await?;
```

### 自定义存储后端

**实现 Storage trait**:
```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn append_agent_message(&self, message: &AgentMessage) -> Result<(), StorageError>;
    async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>, StorageError>;
    async fn list_sessions(&self) -> Result<Vec<String>, StorageError>;
}

// 实现数据库存储
pub struct DatabaseStorage {
    pool: SqlitePool,
}

#[async_trait]
impl Storage for DatabaseStorage {
    // ...
}
```

## 测试策略

### 单元测试

```rust
#[tokio::test]
async fn test_message_bus_publish_subscribe() {
    let bus = MessageBus::new();
    let mut sub = bus.subscribe().await.unwrap();
    
    bus.publish(AgentMessage {
        content: "test".to_string(),
        ..Default::default()
    }).await.unwrap();
    
    let msg = sub.recv().await.unwrap();
    assert_eq!(msg.content, "test");
}
```

### 集成测试

- 完整消息流转测试
- 持久化验证测试
- 多订阅者并发测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
