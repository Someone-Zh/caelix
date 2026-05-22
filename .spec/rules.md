# 项目规范规则

## 约定

### 技术栈
- **编程语言**: Rust (Edition 2021)
- **异步运行时**: tokio (full features)
- **序列化框架**: serde + serde_json + serde_yaml
- **HTTP 框架**: axum 0.8.9 + tower 0.5.3
- **TUI 框架**: ratatui 0.30.0 + crossterm 0.29.0
- **并发容器**: dashmap 6.1.0
- **日志追踪**: tracing 0.1.44 + tracing-subscriber 0.3.23
- **错误处理**: thiserror 1.0.61 + anyhow 1.0.102
- **ID 生成**: uuid 1.8.0 + snowflaked 1.0.3
- **命令行解析**: clap 4.0 (derive feature)
- **定时任务**: cron 0.16.0
- **流式处理**: tokio-stream 0.1.14 + futures 0.3.32
- **嵌入式资源**: rust-embed 8.11.0

### 架构模式
- **分层架构**: 核心定义层 → 运行时层 → 服务层 → 表现层
- **模块化设计**: Workspace 多包架构，13 个独立 crate
- **依赖方向**: 自底向上，严格避免循环依赖
- **接口抽象**: 基于 trait 的面向接口编程
- **插件化扩展**: Hook 机制支持运行时扩展

### 服务方式
- **CLI**: 命令行交互界面（默认模式）
- **HTTP Server**: RESTful API 服务（可选 feature）
- **TUI**: 终端用户图形界面（可选 feature）
- **Library**: 可作为库被其他项目集成

### 涉及框架
- **Agent 系统**: 多 Agent 协作架构（planner、executor、collector 等）
- **工具系统**: 可扩展 Tool trait 实现（文件编辑、搜索、读取等）
- **消息总线**: 发布订阅模式的消息系统
- **任务调度**: 异步任务队列 + 持久化存储
- **配置管理**: 动态加载 + Manager 模式
- **LLM Provider**: 可插拔的 LLM 提供者接口

### 配置管理
- **配置文件位置**: 
  - 默认: `~/.caelix/` (由 `CAELIX_HOME` 环境变量控制)
  - Agent 配置: `$CAELIX_HOME/agents/*.agent`
  - Provider 配置: `$CAELIX_HOME/providers/*.yaml`
  - Skills 配置: `$CAELIX_HOME/skills/*.skill`
  - Commands 配置: `$CAELIX_HOME/commands/*.cmd`
- **配置格式**: YAML + 自定义 `.agent` 格式（frontmatter + markdown）
- **嵌入资源**: 使用 rust-embed 打包默认配置到二进制文件
- **配置热加载**: 支持运行时重新加载配置

## 开发规范

### 命名规则

#### 包命名
- **格式**: `caelix-{module}` 小写，使用连字符分隔
- **示例**: `caelix-api`, `caelix-agent`, `caelix-runtime`
- **原则**: 简洁明了，体现模块职责

#### 文件命名
- **Rust 源文件**: snake_case，如 `runtime_context.rs`, `message_bus.rs`
- **模块入口**: `mod.rs` 或直接在父文件中声明
- **配置文件**: 
  - Agent: `{name}_agent.agent`
  - Skill: `{name}.skill`
  - Command: `{name}.cmd`

#### 类型命名
- **结构体 (struct)**: PascalCase，如 `AgentSpec`, `ChatMessage`, `RuntimeContext`
- **枚举 (enum)**: PascalCase，如 `AgentError`, `MessageRole`, `HookType`
- **Trait**: PascalCase，如 `Tool`, `LlmProvider`, `CaelixApi`, `Hook`
- **类型别名**: PascalCase，如 `BoxStream`, `Pin`

#### 函数和方法命名
- **公开函数**: snake_case，动词开头，如 `create_session`, `chat_stream`, `execute_agent`
- **私有函数**: snake_case，可使用下划线前缀表示内部使用，如 `_internal_helper`
- **构造函数**: `new()`, `with_xxx()`, `from_xxx()`
- **Getter/Setter**: `get_xxx()`, `set_xxx()`, `xxx()` (Rust 惯例省略 get 前缀)
- **布尔判断**: `is_xxx()`, `has_xxx()`, `can_xxx()`，如 `has_tool_calls()`, `is_empty()`

#### 变量命名
- **局部变量**: snake_case，如 `session_id`, `tool_name`, `message_count`
- **常量**: SCREAMING_SNAKE_CASE，如 `MAX_RETRY_COUNT`, `DEFAULT_TIMEOUT`
- **静态变量**: snake_case + 类型说明，如 `ID_GENERATOR`, `HOOK_REGISTRY`
- **Arc/Mutex 包装**: 保持原名或加 `_arc`/`_mutex` 后缀

#### 模块命名
- **目录名**: snake_case，如 `caelix_api/src/agent/`, `caelix_runtime/src/hooks/`
- **模块声明**: 使用 `pub mod xxx;` 在 `lib.rs` 或父模块中声明

### 代码风格

#### 导入规范
```rust
// 标准库导入
use std::sync::Arc;
use std::collections::HashMap;

// 第三方库导入（按字母顺序）
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

// 内部模块导入（按依赖层级）
use caelix_api::error::ApiError;
use caelix_api::agent::AgentOutputChunk;
use crate::types::ChatRequest;
```

#### 可见性控制
- **默认私有**: 除非明确需要公开，否则不添加 `pub`
- **公开接口**: 仅在 lib.rs 中 re-export 需要的类型
- **内部实现**: 使用 `pub(crate)` 限制在 crate 内可见
- **测试专用**: 使用 `#[cfg(test)]` 标记测试代码

#### 注释规范
```rust
/// 文档注释（用于公开 API）
/// 
/// 详细描述函数的功能、参数、返回值和可能的错误
/// 
/// # Examples
/// 
/// ```
/// let result = some_function(arg);
/// ```
pub fn some_function(arg: &str) -> Result<String, ApiError> {
    // 行内注释：解释复杂逻辑
    let processed = process_input(arg);
    
    // TODO: 待优化的性能瓶颈
    optimize_result(processed)
}
```

**要求**:
- 所有公开的 `pub` 函数、结构体、枚举必须有文档注释
- 复杂算法和业务逻辑必须添加行内注释
- 使用 `TODO`、`FIXME`、`NOTE` 标记特殊说明
- 注释语言：优先使用中文，技术术语可保留英文

#### 错误处理
```rust
// 使用 thiserror 定义错误类型
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    ProviderError(String),
    
    #[error("Tool execution failed: {tool_name} - {error}")]
    ToolError { tool_name: String, error: String },
    
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

// 使用 ? 运算符传播错误
async fn execute_tool(&self, input: JsonValue) -> Result<ToolResult, AgentError> {
    let result = self.internal_execute(input).await?;
    Ok(result)
}

// 使用 anyhow 处理非关键错误
async fn optional_operation(&self) -> anyhow::Result<()> {
    // 可能失败但不影响主流程的操作
    self.try_something().await.map_err(|e| anyhow::anyhow!("Optional failed: {}", e))?;
    Ok(())
}
```

**原则**:
- 核心业务逻辑使用 `thiserror` 定义精确的错误类型
- 辅助功能使用 `anyhow::Result` 简化错误处理
- 错误信息必须包含足够的上下文信息
- 避免使用 `.unwrap()` 和 `.expect()`，除非在测试代码中

#### 日志记录
```rust
use tracing::{info, warn, error, debug, trace, instrument};

// 函数级别追踪
#[instrument(skip(self), fields(session_id = %session_id))]
async fn chat_stream(&self, session_id: &str) -> Result<(), ApiError> {
    info!("开始聊天流");
    debug!("会话 ID: {}", session_id);
    
    if condition {
        warn!("检测到异常情况");
    }
    
    error!("发生错误: {:?}", error);
    
    Ok(())
}
```

**级别选择**:
- `ERROR`: 系统错误、异常退出
- `WARN`: 警告信息、降级处理
- `INFO`: 重要业务流程、状态变更
- `DEBUG`: 调试信息、详细流程
- `TRACE`: 最详细的追踪信息

**要求**:
- 所有异步函数使用 `#[instrument]` 宏自动追踪
- 敏感信息（API Key、密码）不得记录到日志
- 日志级别可通过环境变量 `RUST_LOG` 动态调整

#### 异步编程
```rust
use async_trait::async_trait;
use tokio::sync::Mutex;

// Trait 中的异步方法
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, input: JsonValue) -> ToolResult;
}

// 并发执行
let results = futures::future::join_all(tasks).await;

// 共享状态
let shared_state = Arc::new(Mutex::new(State::new()));
```

**原则**:
- 所有 I/O 操作必须使用异步版本
- Trait 中的异步方法使用 `#[async_trait]` 宏
- 避免在异步上下文中使用阻塞操作
- 使用 `tokio::spawn` 创建后台任务时注意生命周期

#### 并发安全
```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use dashmap::DashMap;

// 读写锁保护共享状态
let cache = Arc::new(RwLock::new(HashMap::new()));

// 高并发场景使用 DashMap
let concurrent_map = Arc::new(DashMap::new());

// Arc 包装实现共享所有权
let shared_data = Arc::new(Data::new());
```

**要求**:
- 跨线程共享数据必须使用 `Arc`
- 可变共享状态使用 `RwLock` 或 `Mutex`
- 高并发读场景优先使用 `DashMap`
- 避免死锁：按固定顺序获取锁，使用 `try_lock` 超时机制

### 安全规范

#### 输入校验
```rust
// 字符串长度限制
if user_input.len() > MAX_INPUT_LENGTH {
    return Err(ApiError::InputTooLong);
}

// JSON 解析校验
let parsed: JsonValue = serde_json::from_str(&input)
    .map_err(|e| ApiError::InvalidJson(e.to_string()))?;

// 路径遍历防护
let safe_path = sanitize_path(&user_path)?;
```

**要求**:
- 所有外部输入必须进行长度、格式校验
- 文件路径必须防止目录穿越攻击
- JSON/YAML 解析必须处理错误情况
- SQL/命令注入防护（虽然本项目不使用数据库）

#### 权限验证
```rust
// Session 隔离
if request.session_id != current_session {
    return Err(ApiError::Unauthorized);
}

// Agent 访问控制
if !allowed_agents.contains(&agent_name) {
    return Err(ApiError::AgentNotFound);
}
```

**当前状态**: 
- 项目暂未实现细粒度权限控制
- Session 级别隔离通过唯一 ID 保证
- 未来需添加 RBAC 权限模型

#### 数据脱敏
```rust
// 日志中脱敏
debug!("API Key: {}...", &api_key[..8]);

// 响应中脱敏
response.api_key = Some("****".to_string());
```

**要求**:
- API Key、密码等敏感信息不得明文存储
- 日志中敏感信息必须脱敏
- 配置文件中的密钥建议使用环境变量注入

#### 加密存储
```rust
// 未来需要实现
use ring::aead;

fn encrypt_sensitive_data(data: &[u8]) -> Vec<u8> {
    // 使用 AES-GCM 加密
}
```

**当前状态**:
- 会话数据以明文存储在文件系统
- 建议未来添加加密选项
- 敏感配置应通过环境变量或密钥管理服务提供

#### 资源限制
```rust
// 防止无限循环
const MAX_ITERATIONS: usize = 100;
let mut count = 0;
while count < MAX_ITERATIONS {
    // ...
    count += 1;
}

// 防止内存溢出
if message_buffer.len() > MAX_BUFFER_SIZE {
    return Err(ApiError::BufferOverflow);
}
```

**要求**:
- 所有循环必须有明确的退出条件
- 缓冲区大小必须有限制
- 递归调用必须有深度限制
- 文件读取必须限制最大大小

### 测试规范

#### 单元测试
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_chat_stream() {
        let api = create_test_api();
        let result = api.chat_stream(request).await;
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_agent_spec_creation() {
        let spec = AgentSpec::new("test".to_string(), "prompt".to_string(), vec![]);
        assert_eq!(spec.name, "test");
    }
}
```

**要求**:
- 每个公共函数至少有一个测试用例
- 边界条件和错误情况必须覆盖
- 使用 `#[tokio::test]` 测试异步代码
- 测试代码放在同一文件的 `#[cfg(test)]` 模块中

#### 集成测试
```rust
// tests/integration_test.rs
use caelix_service::CaelixApiImpl;

#[tokio::test]
async fn test_full_chat_flow() {
    // 完整的聊天流程测试
}
```

**要求**:
- 关键业务流程必须有集成测试
- 测试数据使用独立的测试会话
- 测试后清理临时文件和状态

### 性能规范

#### 内存管理
```rust
// 使用 Arc 避免重复克隆
let shared_config = Arc::new(config);

// 及时释放不再使用的资源
drop(large_data);

// 使用迭代器避免中间集合
let result: Vec<_> = items.iter().filter(|x| x.valid).map(|x| x.transform()).collect();
```

#### 异步优化
```rust
// 并发执行独立任务
let (result1, result2) = tokio::join!(task1(), task2());

// 使用缓冲通道避免背压
let (tx, rx) = tokio::sync::mpsc::channel(100);
```

**要求**:
- 避免不必要的克隆操作
- 合理使用缓存减少重复计算
- 监控内存使用，防止内存泄漏
- 大文件处理使用流式读取

### 文档规范

#### README 要求
- 项目简介和架构图
- 快速开始指南
- 核心特性说明
- 开发指南
- 贡献指南

#### API 文档
- 所有公开 API 必须有文档注释
- 包含使用示例
- 说明可能的错误情况
- 标注废弃的 API

#### 变更日志
- 重大变更必须更新 CHANGELOG
- 破坏性变更必须明确标注
- 迁移指南必须清晰

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
