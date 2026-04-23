# 会话管理器重构说明

## 修改概述

将 API 层的独立 `session_manager.rs` 移除，改为使用现有的 `src/runtime/message/manager.rs` 中的 `SessionManager` 来统一管理会话。

## 修改内容

### 1. 扩展 runtime/message/types.rs

在 `SessionState` 中添加了 `config` 字段，用于存储会话配置：

```rust
pub struct SessionState {
    pub active_spans: std::collections::HashMap<String, ActiveSpanInfo>,
    /// 会话配置信息
    pub config: Option<SessionConfig>,
}

/// 会话配置
pub struct SessionConfig {
    pub session_id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### 2. 扩展 runtime/message/manager.rs

在 `SessionManager` 中添加了会话配置管理方法：

- `create_session_config(session_id)`: 创建新会话配置
- `get_session_config(session_id)`: 获取会话配置
- `set_session_provider(session_id, provider)`: 设置会话提供者
- `set_session_model(session_id, model)`: 设置会话模型
- `set_session_agent(session_id, agent)`: 设置会话 agent
- `session_exists(session_id)`: 检查会话是否存在
- `list_sessions()`: 获取所有会话 ID

同时实现了 `Debug` trait 以支持调试输出。

### 3. 更新 config/context.rs

在 `CaelixContext` 中添加了 `session_manager` 字段：

```rust
pub struct CaelixContext {
    pub agent_manager: Arc<AgentManager>,
    pub tool_manager: Arc<ToolManager>,
    pub llm_provider_manager: Arc<RwLock<ProviderManager>>,
    pub session_manager: Arc<SessionManager>,  // 新增
}
```

在 `CaelixContext::new()` 中初始化 SessionManager：

```rust
let bus = MessageBus::new(1024);
let storage = Arc::new(FileStorage::new("./sessions".to_string()));
let session_manager = Arc::new(SessionManager::new(bus, storage));
```

### 4. 更新 runtime/mod.rs

将 `message` 模块从私有改为公开：

```rust
pub mod message;  // 原来是 mod message;
```

### 5. 删除 api/session_manager.rs

移除了之前创建的独立会话管理器文件。

### 6. 更新 api/core.rs

修改 `CaelixApiImpl` 以使用 `context.session_manager`：

- 移除了独立的 `session_manager` 字段
- 所有会话操作都通过 `self.context.session_manager` 进行
- `create_session()` 使用异步方式创建会话配置

### 7. 更新 api/types.rs

- 移除了重复的 `SessionConfig` 定义（现在使用 runtime 的版本）
- 保留了 `ApiError` 及其辅助方法

## 优势

1. **统一管理**: 会话配置和消息管理集中在一个地方
2. **持久化支持**: 利用现有的存储后端，会话配置可以持久化
3. **状态跟踪**: 可以同时跟踪会话的配置和活跃 spans
4. **减少冗余**: 避免了重复的会话管理代码

## 使用示例

```rust
// 创建会话
let session_id = api.create_session();

// 设置会话配置
api.set_session_provider(&session_id, "bailian").await?;
api.set_session_model(&session_id, "qwen-max").await?;

// 获取会话配置
let config = context.session_manager
    .get_session_config(&session_id)
    .await;
```

## 注意事项

1. SessionManager 的初始化需要 MessageBus 和 StorageBackend
2. 会话配置的创建是异步的（在 create_session 中使用 tokio::spawn）
3. SessionState 现在包含两个部分：active_spans 和 config
4. 所有会话操作都是线程安全的（使用 RwLock）
