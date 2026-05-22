# 运行时系统规范

## 功能概述

运行时系统是 Caelix 的核心基础设施，提供 Agent 执行时的上下文环境（RuntimeContext）、扩展机制（Hook 系统）和唯一 ID 生成服务。它是连接 Agent 引擎与底层服务的桥梁，确保 Agent 在执行过程中能够访问必要的资源和状态。

## 核心能力

### 1. RuntimeContext（运行时上下文）

**职责**:
- 携带请求链路信息（session_id, request_id, span_id, trace_id）
- 提供会话隔离机制
- 传递共享资源（MessageBus、TaskManager 等）
- 支持快照和恢复

**结构定义**:
```rust
pub struct RuntimeContext {
    session_id: String,
    request_id: String,
    span_id: String,
    trace_id: String,
    message_bus: Arc<MessageBus>,
    task_manager: Arc<TaskManager>,
    agent_manager: Arc<AgentManager>,
    tool_manager: Arc<ToolManager>,
}
```

**关键方法**:
```rust
impl RuntimeContext {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            request_id: generate_request_id(),
            span_id: generate_span_id(),
            trace_id: generate_trace_id(),
            // ... 初始化其他字段
        }
    }
    
    pub fn session_id(&self) -> &str { &self.session_id }
    pub fn request_id(&self) -> &str { &self.request_id }
    pub fn span_id(&self) -> &str { &self.span_id }
    pub fn trace_id(&self) -> &str { &self.trace_id }
    
    pub fn message_bus(&self) -> &Arc<MessageBus> { &self.message_bus }
    pub fn task_manager(&self) -> &Arc<TaskManager> { &self.task_manager }
    
    // 创建子 Span（用于嵌套调用）
    pub fn create_child_span(&self) -> Self {
        let mut child = self.clone();
        child.span_id = generate_span_id();
        child
    }
    
    // 快照（用于 Hook 执行前后保存状态）
    pub fn snapshot(&self) -> RuntimeContextSnapshot {
        RuntimeContextSnapshot {
            session_id: self.session_id.clone(),
            request_id: self.request_id.clone(),
            span_id: self.span_id.clone(),
        }
    }
}
```

**使用示例**:
```rust
// 在 API 层创建上下文
let context = RuntimeContext::new(session_id.clone());

// 传递给 Agent 执行器
execute_agent_with_messaging(&agent_spec, messages, &context).await?;

// Agent 内部使用上下文
context.message_bus().publish(message).await?;
context.task_manager().create_task(task).await?;
```

### 2. Hook 系统

**Hook 类型**:
```rust
pub enum HookType {
    BeforeAgent,   // Agent 执行前
    AfterAgent,    // Agent 执行后
    BeforeTool,    // 工具执行前
    AfterTool,     // 工具执行后
    OnError,       // 错误发生时
}
```

**Hook Trait**:
```rust
#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn hook_type(&self) -> HookType;
    fn priority(&self) -> u32; // 优先级，数值越小越先执行
    
    async fn execute(&self, context: &HookContext) -> Result<(), AgentError>;
    
    // 条件匹配：决定是否应用此 Hook
    fn should_apply(&self, agent_name: &str, agent_group: Option<&str>) -> bool;
}
```

**HookContext**:
```rust
pub struct HookContext {
    pub session_id: String,
    pub agent_name: Option<String>,
    pub message: Option<String>,
    pub tool_name: Option<String>,
    pub runtime_context: Arc<RuntimeContext>,
}
```

**HookRegistry**:
```rust
pub struct HookRegistry {
    hooks: DashMap<HookType, Vec<Arc<dyn Hook>>>,
}

impl HookRegistry {
    pub fn register(&self, hook_type: HookType, hook: Arc<dyn Hook>) {
        let mut hooks = self.hooks.entry(hook_type).or_insert_with(Vec::new);
        hooks.push(hook);
        // 按优先级排序
        hooks.sort_by_key(|h| h.priority());
    }
    
    pub async fn execute_before_agent(
        &self,
        agent_name: &str,
        agent_group: Option<&str>,
        context: &RuntimeContext,
    ) -> Result<(), AgentError> {
        let hooks = self.hooks.get(&HookType::BeforeAgent);
        if let Some(hooks) = hooks {
            for hook in hooks.iter() {
                if hook.should_apply(agent_name, agent_group) {
                    let hook_ctx = HookContext {
                        session_id: context.session_id().to_string(),
                        agent_name: Some(agent_name.to_string()),
                        runtime_context: Arc::new(context.clone()),
                        ..Default::default()
                    };
                    hook.execute(&hook_ctx).await?;
                }
            }
        }
        Ok(())
    }
}
```

### 3. 内置 Hook

#### SkillHook（技能加载）

**职责**: 根据 Agent 名称和 group 自动加载相关技能文档，注入到 system prompt

**实现**:
```rust
pub struct SkillHook {
    skill_manager: Arc<SkillManager>,
}

#[async_trait]
impl Hook for SkillHook {
    fn name(&self) -> &str { "skill_hook" }
    fn hook_type(&self) -> HookType { HookType::BeforeAgent }
    fn priority(&self) -> u32 { 100 } // 高优先级，最先执行
    
    async fn execute(&self, context: &HookContext) -> Result<(), AgentError> {
        if let Some(agent_name) = &context.agent_name {
            // 加载匹配的技能
            let skills = self.skill_manager
                .load_skills_for_agent(agent_name)
                .await?;
            
            // 将技能内容添加到 system prompt
            if !skills.is_empty() {
                let skill_content = skills.join("\n\n");
                // 通过某种方式注入到 Agent 上下文
                inject_skill_prompt(&skill_content);
            }
        }
        Ok(())
    }
    
    fn should_apply(&self, _agent_name: &str, _agent_group: Option<&str>) -> bool {
        true // 对所有 Agent 生效
    }
}
```

#### MessageBusHook（消息记录）

**职责**: 在 Agent 开始和结束时发送通知消息到消息总线

**实现**:
```rust
pub struct MessageBusHook;

#[async_trait]
impl Hook for MessageBusHook {
    fn name(&self) -> &str { "message_bus_hook" }
    fn hook_type(&self) -> HookType { HookType::BeforeAgent }
    fn priority(&self) -> u32 { 200 }
    
    async fn execute(&self, context: &HookContext) -> Result<(), AgentError> {
        let notification = NotificationMessage {
            session_id: context.session_id.clone(),
            r#type: NotificationType::AgentStart,
            content: format!("Agent {} started", context.agent_name.as_deref().unwrap_or("unknown")),
            timestamp: Utc::now(),
            ..Default::default()
        };
        
        context.runtime_context
            .message_bus()
            .publish_notification(notification)
            .await?;
        
        Ok(())
    }
}
```

#### ToolResultCheckHook（工具结果检查）

**职责**: 在工具执行后检查结果，检测异常情况

**实现**:
```rust
pub struct ToolResultCheckHook;

#[async_trait]
impl Hook for ToolResultCheckHook {
    fn name(&self) -> &str { "tool_result_check_hook" }
    fn hook_type(&self) -> HookType { HookType::AfterTool }
    fn priority(&self) -> u32 { 50 }
    
    async fn execute(&self, context: &HookContext) -> Result<(), AgentError> {
        // 检查工具执行结果
        if let Some(tool_result) = get_last_tool_result() {
            if let Some(error) = &tool_result.error {
                warn!("Tool execution error: {}", error);
                
                // 发送警告通知
                let notification = NotificationMessage {
                    session_id: context.session_id.clone(),
                    r#type: NotificationType::Warning,
                    content: format!("Tool {} failed: {}", 
                        context.tool_name.as_deref().unwrap_or("unknown"), error),
                    timestamp: Utc::now(),
                    ..Default::default()
                };
                
                context.runtime_context
                    .message_bus()
                    .publish_notification(notification)
                    .await?;
            }
        }
        Ok(())
    }
}
```

### 4. ID 生成器

**Snowflake 算法**:
```rust
use snowflaked::Generator;

static ID_GENERATOR: Lazy<Mutex<Generator>> = Lazy::new(|| {
    Mutex::new(Generator::new(1, 1))
});

pub fn generate_session_id() -> String {
    let id = ID_GENERATOR.lock().unwrap().generate();
    format!("sess_{}", id)
}

pub fn generate_request_id() -> String {
    let id = ID_GENERATOR.lock().unwrap().generate();
    format!("req_{}", id)
}

pub fn generate_span_id() -> String {
    let id = ID_GENERATOR.lock().unwrap().generate();
    format!("span_{}", id)
}

pub fn generate_task_id() -> String {
    let id = ID_GENERATOR.lock().unwrap().generate();
    format!("task_{}", id)
}

pub fn generate_trace_id() -> String {
    let id = ID_GENERATOR.lock().unwrap().generate();
    format!("trace_{}", id)
}
```

**ID 格式**:
- `session_id`: `sess_{snowflake_id}`
- `request_id`: `req_{snowflake_id}`
- `span_id`: `span_{snowflake_id}`
- `task_id`: `task_{snowflake_id}`
- `trace_id`: `trace_{snowflake_id}`

**特性**:
- 全局唯一
- 时间有序
- 分布式友好
- 高性能（无锁或细粒度锁）

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **RuntimeContext** | `caelix-runtime/src/context/runtime_context.rs` | 运行时上下文实现 |
| **HookRegistry** | `caelix-runtime/src/hooks/mod.rs` | Hook 注册和管理 |
| **SkillHook** | `caelix-runtime/src/hooks/skill_hook.rs` | 技能加载 Hook |
| **MessageBusHook** | `caelix-runtime/src/hooks/message_bus_hook.rs` | 消息记录 Hook |
| **ToolResultCheckHook** | `caelix-runtime/src/hooks/tool_result_check_hook.rs` | 工具结果检查 Hook |
| **HookLoader** | `caelix-runtime/src/hooks/loader.rs` | Hook 配置加载器 |
| **IdGenerator** | `caelix-runtime/src/id_generator.rs` | ID 生成器 |

### 初始化流程

```rust
// caelix-config/src/lib.rs
pub async fn initialize_runtime() -> Result<RuntimeContext, ApiError> {
    // 1. 创建消息总线
    let message_bus = Arc::new(MessageBus::new());
    
    // 2. 创建任务管理器
    let task_manager = Arc::new(TaskManager::new());
    
    // 3. 创建 Hook 注册表
    let hook_registry = Arc::new(HookRegistry::new());
    
    // 4. 注册内置 Hook
    hook_registry.register(
        HookType::BeforeAgent,
        Arc::new(SkillHook::new(skill_manager)),
    );
    hook_registry.register(
        HookType::BeforeAgent,
        Arc::new(MessageBusHook),
    );
    hook_registry.register(
        HookType::AfterTool,
        Arc::new(ToolResultCheckHook),
    );
    
    // 5. 创建运行时上下文
    let session_id = generate_session_id();
    let context = RuntimeContext::new(session_id)
        .with_message_bus(message_bus)
        .with_task_manager(task_manager)
        .with_hook_registry(hook_registry);
    
    Ok(context)
}
```

### Hook 执行流程

```
Agent 执行请求
      ↓
HookRegistry::execute_before_agent()
      ↓
遍历 BeforeAgent Hooks（按优先级）
  ├─ SkillHook (priority=100)
  │   └─ 加载技能文档
  ├─ MessageBusHook (priority=200)
  │   └─ 发送 AgentStart 通知
  └─ 自定义 Hooks...
      ↓
Agent 执行
      ↓
HookRegistry::execute_after_agent()
      ↓
遍历 AfterAgent Hooks
  └─ 自定义 Hooks...
      ↓
返回结果
```

## 会话隔离

### 隔离机制

**每个请求独立的 RuntimeContext**:
```rust
// API 层为每个请求创建新的上下文
async fn handle_chat_request(request: ChatRequest) -> Result<..., ApiError> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);
    let context = RuntimeContext::new(session_id)
        .with_request_id(generate_request_id())
        .with_span_id(generate_span_id());
    
    // 所有后续操作都使用这个上下文
    execute_agent(&context).await?;
    
    Ok(response)
}
```

**消息隔离**:
```rust
// 消息自动携带 session_id
let message = AgentMessage {
    session_id: context.session_id().to_string(),
    // ...
};

// SessionManager 按 session_id 过滤和存储
session_manager.on_message(message).await;
```

**任务隔离**:
```rust
// 任务关联到特定会话
let task = TaskMeta {
    session_id: context.session_id().to_string(),
    // ...
};

// 查询时按 session_id 过滤
let tasks = task_manager.list_tasks(Some(context.session_id())).await?;
```

## 扩展指南

### 添加自定义 Hook

**步骤**:

1. **实现 Hook trait**
```rust
#[derive(Debug)]
pub struct MyCustomHook;

#[async_trait]
impl Hook for MyCustomHook {
    fn name(&self) -> &str {
        "my_custom_hook"
    }
    
    fn hook_type(&self) -> HookType {
        HookType::BeforeAgent
    }
    
    fn priority(&self) -> u32 {
        150 // 介于 SkillHook 和 MessageBusHook 之间
    }
    
    async fn execute(&self, context: &HookContext) -> Result<(), AgentError> {
        // 实现自定义逻辑
        info!("Custom hook executed for agent: {:?}", context.agent_name);
        Ok(())
    }
    
    fn should_apply(&self, agent_name: &str, _agent_group: Option<&str>) -> bool {
        // 只对特定 Agent 生效
        agent_name == "planner_agent"
    }
}
```

2. **注册 Hook**
```rust
// caelix-runtime/src/hooks/mod.rs
pub fn register_default_hooks(registry: &Arc<HookRegistry>) {
    registry.register(HookType::BeforeAgent, Arc::new(MyCustomHook));
}
```

3. **配置 Hook（可选）**
```yaml
# hooks.yaml
hooks:
  - name: my_custom_hook
    type: before_agent
    enabled: true
    config:
      param1: value1
```

### 扩展 RuntimeContext

**添加新字段**:
```rust
pub struct RuntimeContext {
    // 现有字段...
    custom_field: Option<String>,
}

impl RuntimeContext {
    pub fn with_custom_field(mut self, value: String) -> Self {
        self.custom_field = Some(value);
        self
    }
    
    pub fn custom_field(&self) -> Option<&str> {
        self.custom_field.as_deref()
    }
}
```

## 性能优化

### 1. 上下文克隆优化

**使用 Arc 共享不可变数据**:
```rust
pub struct RuntimeContext {
    message_bus: Arc<MessageBus>,      // 共享，不克隆
    task_manager: Arc<TaskManager>,    // 共享，不克隆
    session_id: String,                // 每个请求独立
    request_id: String,                // 每个请求独立
}
```

### 2. Hook 执行优化

**并行执行独立 Hook**:
```rust
// 如果多个 Hook 互不依赖，可以并行执行
let results = futures::future::join_all(
    hooks.iter().map(|hook| hook.execute(&context))
).await;
```

**缓存 Hook 匹配结果**:
```rust
// 缓存 Agent 匹配的 Hook 列表
let matched_hooks = hook_cache
    .get_or_insert_with(agent_name, || {
        registry.get_matching_hooks(agent_name)
    });
```

### 3. ID 生成优化

**批量预生成 ID**:
```rust
struct IdPool {
    ids: Mutex<Vec<u64>>,
}

impl IdPool {
    fn get_id(&self) -> u64 {
        let mut ids = self.ids.lock().unwrap();
        if ids.is_empty() {
            // 批量生成 100 个 ID
            for _ in 0..100 {
                ids.push(generator.generate());
            }
        }
        ids.pop().unwrap()
    }
}
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `ContextCreationFailed` | 上下文创建失败 | 重试或返回错误 |
| `HookExecutionFailed` | Hook 执行失败 | 记录日志，继续执行或中断 |
| `IdGenerationFailed` | ID 生成器故障 | 降级使用 UUID |
| `HookNotFound` | Hook 未找到 | 跳过该 Hook |

### Hook 错误隔离

```rust
// 单个 Hook 失败不影响其他 Hook
for hook in hooks {
    match hook.execute(&context).await {
        Ok(_) => continue,
        Err(e) => {
            error!("Hook {} failed: {:?}", hook.name(), e);
            // 可选：是否中断后续 Hook 执行
            if hook.critical() {
                return Err(e);
            }
        }
    }
}
```

## 测试策略

### 单元测试

```rust
#[tokio::test]
async fn test_runtime_context_creation() {
    let context = RuntimeContext::new("sess_test".to_string());
    assert_eq!(context.session_id(), "sess_test");
    assert!(!context.request_id().is_empty());
}

#[tokio::test]
async fn test_hook_execution_order() {
    let registry = HookRegistry::new();
    registry.register(HookType::BeforeAgent, Arc::new(LowPriorityHook));
    registry.register(HookType::BeforeAgent, Arc::new(HighPriorityHook));
    
    let context = create_test_context();
    registry.execute_before_agent("test_agent", None, &context).await.unwrap();
    
    // 验证 HighPriorityHook 先于 LowPriorityHook 执行
}
```

### 集成测试

- 完整请求链路测试（API → RuntimeContext → Agent → Hook）
- 会话隔离测试
- Hook 链执行测试
- ID 生成唯一性测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
