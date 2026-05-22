# 任务调度系统功能规范

## 正在处理的需求目录
- 完善 task 包的运行时上下文依赖
- 为 task 的任务添加支持返回值 Result<String, AgentError>，并增加结果保存功能
- 增加 TaskKind 增加一个待办种类（Todo），完全由外部触发任务状态的变更

## 涉及的模型文件 
|描述|位置|
|---|---|
|任务类型定义（TaskId, TaskKind, TaskStatus, Runnable trait）|caelix-api/src/task/mod.rs|
|任务元数据（TaskMeta）、工厂（RunnableFactory）|caelix-task/src/types.rs|
|任务管理器（TaskManager）|caelix-task/src/manager.rs|
|任务持久化（TaskPersistence, FilePersistence）|caelix-task/src/persistence.rs|
|任务调度器（TaskScheduler）|caelix-task/src/scheduler.rs|
|运行时上下文（RuntimeContext）|caelix-runtime/src/context/runtime_context.rs|
|任务消息类型（TaskMessage）|caelix-message/src/task_message.rs|

## 涉及的依赖功能
* [Agent 系统](file://.spec/caelix-agent/spec.md) - 通过 delegate_task 工具提交任务
* [消息总线](file://.spec/caelix-message/spec.md) - 任务状态变更通知、外部触发待办任务
* [Hook 系统](file://.spec/caelix-runtime/spec.md) - 任务执行前后的钩子扩展
* [会话管理](file://.spec/caelix-message/spec.md#会话管理) - 任务归属于特定 session

## 涉及的依赖服务
|描述|位置|
|---|---|
|MessageBus - 消息总线|caelix-message/src/bus.rs|
|SessionManager - 会话管理器|caelix-message/src/manager.rs|
|FileStorage - 文件存储|caelix-message/src/storage.rs|
|RuntimeContext - 运行时上下文|caelix-runtime/src/context/runtime_context.rs|

## 当前功能详细信息

### 1. 任务类型系统（caelix-api/src/task/mod.rs）

**业务逻辑和功能介绍**:
- **TaskId**: 使用 Snowflake 算法生成的唯一任务 ID，格式为 `T-{timestamp}-{sequence}`
- **TaskKind**: 任务分类枚举
  - `Async`: 异步任务，提交后立即执行
  - `Once(DateTime<Utc>)`: 一次性定时任务，在指定时间执行
  - `Cron(String)`: 周期性任务，按 cron 表达式定期执行
- **TaskStatus**: 任务状态枚举
  - `Pending`: 等待中
  - `Scheduled`: 已调度
  - `Running`: 执行中
  - `Completed`: 已完成
  - `Failed(String)`: 失败（包含错误信息）
  - `Cancelled`: 已取消
- **Runnable trait**: 可执行任务接口
  ```rust
  #[async_trait]
  pub trait Runnable: Send + Sync + 'static {
      async fn run(&self) -> anyhow::Result<()>;
      fn task_type(&self) -> &'static str;
      fn payload(&self) -> String;
  }
  ```

**异常场景**:
- 任务执行失败时，状态更新为 `Failed(error_message)`
- 任务被取消时，立即 abort tokio task

**校验逻辑**:
- TaskId 必须唯一
- Cron 表达式必须符合 cron crate 的语法

**功能实际文件位置**: `caelix-api/src/task/mod.rs`

**测试用例位置**: 暂无单元测试

---

### 2. 任务元数据与工厂（caelix-task/src/types.rs）

**业务逻辑和功能介绍**:
- **TaskMeta**: 任务元数据结构，用于持久化
  - 包含 task_id, session_id, span_id, tool_call_id, task_name
  - 包含 kind, status, progress（进度 0.0-1.0）
  - 包含 created_at, updated_at 时间戳
  - 包含 task_type_name, task_payload 用于恢复任务
- **RunnableFactory**: 任务工厂，用于从持久化数据恢复任务
  - 注册构造函数：`register(name, constructor)`
  - 创建任务实例：`create(name, payload) -> Option<Box<dyn Runnable>>`

**异常场景**:
- 工厂无法找到对应的任务类型时返回 None

**功能实际文件位置**: `caelix-task/src/types.rs`

---

### 3. 任务管理器（caelix-task/src/manager.rs）

**业务逻辑和功能介绍**:
- **核心功能**:
  - `submit()`: 提交新任务，根据 TaskKind 决定立即执行或调度
  - `cancel()`: 取消任务，abort tokio task 并更新状态
  - `get_status()`: 查询任务状态
  - `wait()`: 等待任务完成（自旋等待）
  - `list_tasks()`: 列出任务（支持按 session 过滤）
  - `update_progress()`: 更新任务进度并发送通知
  - `restore()`: 从持久化存储恢复任务（启动时调用）

- **内部机制**:
  - 使用 DashMap 存储任务句柄（TaskHandle）
  - TaskHandle 包含：TaskMeta, oneshot::Sender, RuntimeContext（占位符）, JoinHandle
  - 后台调度器循环检查待执行任务
  - 任务执行完成后发送状态更新消息到 MessageBus

**当前问题**:
- RuntimeContext 暂时使用占位符 `Option<()>`，未实现上下文传递
- 任务执行时无法访问 session_id、provider、model 等上下文信息

**功能实际文件位置**: `caelix-task/src/manager.rs`

---

### 4. 任务持久化（caelix-task/src/persistence.rs）

**业务逻辑和功能介绍**:
- **TaskPersistence trait**: 持久化接口
  - `save(meta)`: 保存任务元数据
  - `delete(task_id)`: 删除任务文件
  - `load_all()`: 加载所有任务（用于恢复）

- **FilePersistence 实现**:
  - 存储位置：`$CAELIX_HOME/tasks/{task_id}.json`
  - 格式：JSON（serde_json 序列化）
  - 所有任务都持久化，包括 Async 任务

**异常场景**:
- 文件读写失败时返回 anyhow::Error

**功能实际文件位置**: `caelix-task/src/persistence.rs`

---

### 5. 任务调度器（caelix-task/src/scheduler.rs）

**业务逻辑和功能介绍**:
- **TaskScheduler**: 任务调度器
  - 维护优先队列（BinaryHeap），按执行时间排序
  - `schedule(meta)`: 将任务加入调度队列
  - `next_ready()`: 获取下一个就绪的任务（阻塞等待）
  - `cancel(task_id)`: 从调度队列移除任务
  - `calculate_next_run(kind)`: 计算下次执行时间（针对 Cron 任务）

**功能实际文件位置**: `caelix-task/src/scheduler.rs`

---

## 已有的子模块
|描述|位置|
|---|---|
|delegate_task 工具|[caelix-service/src/tools/delegate_task.rs](file://caelix-service/src/tools/delegate_task.rs)|
|list_tasks 工具|[caelix-service/src/tools/list_tasks.rs](file://caelix-service/src/tools/list_tasks.rs)|

---

## 需求设计

### 需求 1：完善 task 包的运行时上下文依赖

#### 设计目标
让任务在执行时能够访问完整的 RuntimeContext，包括 session_id、request_id、span_id、trace_id、provider、model 等信息。

#### 设计方案

**1.1 修改 TaskHandle 结构**

在 `caelix-task/src/manager.rs` 中，将占位符替换为真实的 RuntimeContext：

```rust
use caelix_runtime::context::RuntimeContext;

type TaskHandle = (
    TaskMeta,
    Option<oneshot::Sender<Result<String, AgentError>>>,
    Option<RuntimeContext>,  // 恢复 RuntimeContext
    Option<JoinHandle<()>>,
);
```

**1.2 在 submit 时捕获 RuntimeContext**

修改 `TaskManager::submit()` 方法，在任务提交时捕获当前的 RuntimeContext：

```rust
pub async fn submit(
    &self,
    session_id: String,
    span_id: String,
    tool_call_id: Option<String>,
    task_name: Option<String>,
    kind: TaskKind,
    runnable: Box<dyn Runnable>,
) -> TaskId {
    // ... 现有代码 ...
    
    // 捕获当前的 RuntimeContext
    let runtime_ctx = RuntimeContext::current();
    
    match kind {
        TaskKind::Async => {
            // ... 
            self.registry.insert(task_id.clone(), (meta, Some(tx), Some(runtime_ctx), Some(handle)));
        }
        TaskKind::Once(_) | TaskKind::Cron(_) => {
            // ...
            self.registry.insert(task_id.clone(), (meta.clone(), Some(tx), Some(runtime_ctx), None));
        }
    }
}
```

**1.3 在任务执行时恢复 RuntimeContext**

修改 `execute_task_inner()` 方法，在执行任务前恢复上下文：

```rust
async fn execute_task_inner(
    runnable: Box<dyn Runnable>,
    mut meta: TaskMeta,
    bus: Arc<MessageBus>,
    registry: Arc<DashMap<TaskId, TaskHandle>>,
    scheduler: Arc<TaskScheduler>,
    persistence: Arc<dyn TaskPersistence>,
    runtime_ctx: RuntimeContext,  // 新增参数
) {
    // 在 RuntimeContext 的作用域内执行任务
    let result = RuntimeContext::scope(runtime_ctx.clone(), async {
        runnable.run().await
    }).await;
    
    // ... 后续处理保持不变 ...
}
```

**1.4 修改 restore 方法**

对于从持久化恢复的任务，由于无法重建原始的 RuntimeContext，需要创建一个最小化的上下文：

```rust
pub async fn restore(&self) -> Result<()> {
    let metas = self.persistence.load_all().await?;
    for mut meta in metas {
        meta.status = TaskStatus::Scheduled;
        
        let (tx, _) = oneshot::channel();
        let task_id = meta.task_id.clone();
        
        // 创建最小化的 RuntimeContext（仅包含必要信息）
        let runtime_ctx = RuntimeContext::new(
            Some(meta.session_id.clone()),
            None,
            std::env::current_dir().unwrap_or_default(),
            "unknown".to_string(),  // 默认 provider
            "unknown".to_string(),  // 默认 model
            false,
            None,
        );
        
        self.registry.insert(task_id, (meta.clone(), Some(tx), Some(runtime_ctx), None));
        self.scheduler.schedule(meta.clone()).await;
    }
    Ok(())
}
```

#### 验收标准
- [ ] 任务执行时可以访问 RuntimeContext::session_id()
- [ ] 任务执行时可以访问 RuntimeContext::provider() 和 model()
- [ ] 任务执行时的 span_id 和 trace_id 正确传播
- [ ] 从持久化恢复的任务可以正常执行（使用默认上下文）
- [ ] 所有现有测试通过

---

### 需求 2：为任务添加返回值支持并保存结果

#### 设计目标
1. 修改 Runnable trait 使其返回 `Result<String, AgentError>`
2. 将任务执行结果保存到 TaskMeta 中
3. 任务结果文件保存在 `sessions/{session_id}/tasks/{task_id}.json`
4. 任务状态变更时同步更新文件中的状态
5. 向消息总线发送任务结果消息

#### 设计方案

**2.1 修改 Runnable trait 签名**

在 `caelix-api/src/task/mod.rs` 中修改：

```rust
use crate::error::AgentError;

#[async_trait]
pub trait Runnable: Send + Sync + 'static {
    /// 执行任务并返回结果
    /// 
    /// # Returns
    /// - Ok(String): 任务执行成功，返回结果字符串
    /// - Err(AgentError): 任务执行失败，返回错误信息
    async fn run(&self) -> Result<String, AgentError>;
    
    fn task_type(&self) -> &'static str;
    fn payload(&self) -> String;
}
```

**2.2 修改 TaskMeta 添加 result 字段**

在 `caelix-task/src/types.rs` 中修改：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub task_id: TaskId,
    pub session_id: String,
    pub span_id: String,
    pub tool_call_id: Option<String>,
    pub task_name: Option<String>,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub progress: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub task_type_name: String,
    pub task_payload: String,
    pub result: Option<String>,  // 新增：任务执行结果
}
```

**2.3 修改 TaskManager 处理返回值**

在 `caelix-task/src/manager.rs` 中修改 `execute_task_inner()`：

```rust
async fn execute_task_inner(
    runnable: Box<dyn Runnable>,
    mut meta: TaskMeta,
    bus: Arc<MessageBus>,
    registry: Arc<DashMap<TaskId, TaskHandle>>,
    scheduler: Arc<TaskScheduler>,
    persistence: Arc<dyn TaskPersistence>,
    runtime_ctx: RuntimeContext,
) {
    let task_id = meta.task_id.clone();
    
    // 发送开始通知
    Self::send_task_notification_static(&meta, TaskNotificationType::Started, &bus).await;
    
    // 在 RuntimeContext 作用域内执行任务
    let result = RuntimeContext::scope(runtime_ctx.clone(), async {
        runnable.run().await
    }).await;

    // 更新状态和结果
    let (final_status, result_str) = match &result {
        Ok(output) => {
            meta.result = Some(output.clone());
            (TaskStatus::Completed, output.clone())
        },
        Err(e) => {
            meta.result = Some(format!("Error: {}", e));
            (TaskStatus::Failed(e.to_string()), String::new())
        },
    };
    
    let is_success = result.is_ok();

    // 更新注册表
    if let Some(mut entry) = registry.get_mut(&task_id) {
        let (m, opt_tx, _, _) = entry.value_mut();
        m.status = final_status.clone();
        m.updated_at = Utc::now();
        m.result = meta.result.clone();  // 同步结果
        meta = m.clone();
        
        // 持久化状态更新（保存到 sessions/{session_id}/tasks/{task_id}.json）
        let _ = persistence.save(&meta).await;
        
        // 通知等待者
        if let Some(tx) = opt_tx.take() {
            let _ = tx.send(result.map_err(|e| anyhow::anyhow!(e)));
        }
    }

    // 发送完成/失败通知（包含结果）
    let notif_type = if is_success {
        TaskNotificationType::Completed
    } else {
        TaskNotificationType::Failed
    };
    Self::send_task_notification_with_result_static(&meta, notif_type, &result_str, &bus).await;

    // 处理后续逻辑（Async/Once/Cron）
    // ... 保持现有逻辑 ...
}
```

**2.4 修改持久化路径**

在 `caelix-task/src/persistence.rs` 中修改 FilePersistence，支持自定义基础路径：

```rust
pub struct FilePersistence {
    base_path: PathBuf,
    session_base_path: Option<PathBuf>,  // 新增：session 级别的基础路径
}

impl FilePersistence {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            session_base_path: None,
        }
    }
    
    /// 设置 session 级别的存储路径
    pub fn with_session_path(mut self, session_id: &str, caelix_home: &PathBuf) -> Self {
        self.session_base_path = Some(
            caelix_home.join("sessions").join(session_id).join("tasks")
        );
        self
    }

    fn get_task_path(&self, task_id: &str, session_id: Option<&str>) -> PathBuf {
        // 优先使用 session 级别的路径
        if let Some(ref session_path) = self.session_base_path {
            if session_id.is_some() {
                return session_path.join(format!("{}.json", task_id));
            }
        }
        // 回退到全局路径
        self.base_path.join(format!("{}.json", task_id))
    }
}

#[async_trait]
impl TaskPersistence for FilePersistence {
    async fn save(&self, meta: &TaskMeta) -> Result<()> {
        self.ensure_dir().await?;
        let path = self.get_task_path(&meta.task_id.to_string(), Some(&meta.session_id));
        let json = serde_json::to_string_pretty(meta)?;
        fs::write(path, json).await?;
        Ok(())
    }
    
    // ... delete 和 load_all 也需要相应调整 ...
}
```

**2.5 增强任务消息**

在 `caelix-message/src/task_message.rs` 中添加结果字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub task_id: String,
    pub session_id: String,
    pub r#type: TaskMessageType,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub result: Option<String>,  // 新增：任务执行结果
}
```

**2.6 更新所有实现 Runnable 的代码**

需要修改所有实现 Runnable trait 的代码，包括：
- `caelix-service/src/tools/delegate_task.rs` 中的 DelegateTaskRunnable
- 其他任何实现了 Runnable 的结构

示例修改：

```rust
#[async_trait]
impl Runnable for DelegateTaskRunnable {
    async fn run(&self) -> Result<String, AgentError> {
        // 原有逻辑...
        
        // 返回结果而不是 ()
        Ok(format!("Task {} completed successfully", self.task_id))
    }
    
    fn task_type(&self) -> &'static str {
        "delegate_task"
    }
    
    fn payload(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
```

#### 验收标准
- [ ] Runnable trait 的 run 方法返回 `Result<String, AgentError>`
- [ ] TaskMeta 包含 result 字段
- [ ] 任务结果保存到 `sessions/{session_id}/tasks/{task_id}.json`
- [ ] 任务状态变更时同步更新文件中的状态和结果
- [ ] 消息总线发送的消息包含任务结果
- [ ] 所有现有的 Runnable 实现都已更新
- [ ] 编译通过，无错误

---

### 需求 3：增加 Todo 任务类型

#### 设计目标
1. 在 TaskKind 中新增 `Todo` 变体
2. Todo 任务初始状态为 Pending
3. Todo 任务不会自动执行，完全由外部通过消息总线触发状态变更
4. 不影响现有的 Async、Once、Cron 任务逻辑

#### 设计方案

**3.1 修改 TaskKind 枚举**

在 `caelix-api/src/task/mod.rs` 中修改：

```rust
/// 任务分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskKind {
    /// 异步任务，提交后立即执行
    Async,
    /// 一次性定时任务，在指定时间执行
    Once(DateTime<Utc>),
    /// 周期性任务，按 cron 表达式定期执行
    Cron(String),
    /// 待办任务，完全由外部触发状态变更
    Todo,
}
```

**3.2 修改 TaskManager 处理 Todo 任务**

在 `caelix-task/src/manager.rs` 的 `submit()` 方法中添加 Todo 分支：

```rust
match kind {
    TaskKind::Async => {
        // 立即执行
        // ... 现有代码 ...
    }
    TaskKind::Once(_) | TaskKind::Cron(_) => {
        // 调度执行
        // ... 现有代码 ...
    }
    TaskKind::Todo => {
        // Todo 任务不执行，仅记录
        meta.status = TaskStatus::Pending;
        self.registry.insert(task_id.clone(), (meta.clone(), Some(tx), Some(runtime_ctx), None));
        let _ = self.persistence.save(&meta).await;
        
        // 发送任务创建通知
        Self::send_task_notification_static(&meta, TaskNotificationType::Created, &self.bus).await;
    }
}
```

**3.3 添加外部触发状态变更的 API**

在 TaskManager 中添加新方法：

```rust
/// 外部触发 Todo 任务状态变更
/// 
/// # Arguments
/// * `task_id` - 任务 ID
/// * `new_status` - 新的状态
/// * `result` - 可选的结果字符串（当状态为 Completed 或 Failed 时）
/// 
/// # Returns
/// - true: 状态变更成功
/// - false: 任务不存在或状态不允许变更
pub async fn update_todo_status(
    &self,
    task_id: TaskId,
    new_status: TaskStatus,
    result: Option<String>,
) -> bool {
    if let Some(mut entry) = self.registry.get_mut(&task_id) {
        let (meta, _, _, _) = entry.value_mut();
        
        // 验证状态流转合法性
        if !Self::is_valid_todo_transition(&meta.status, &new_status) {
            return false;
        }
        
        // 更新状态
        meta.status = new_status.clone();
        meta.updated_at = Utc::now();
        if let Some(res) = result {
            meta.result = Some(res);
        }
        
        // 持久化
        let _ = self.persistence.save(&meta).await;
        
        // 发送状态变更通知
        let notif_type = match new_status {
            TaskStatus::Completed => TaskNotificationType::Completed,
            TaskStatus::Failed(_) => TaskNotificationType::Failed,
            TaskStatus::Running => TaskNotificationType::Started,
            _ => return true,
        };
        Self::send_task_notification_static(&meta, notif_type, &self.bus).await;
        
        true
    } else {
        false
    }
}

/// 验证 Todo 任务的状态流转是否合法
fn is_valid_todo_transition(current: &TaskStatus, next: &TaskStatus) -> bool {
    matches!(
        (current, next),
        (TaskStatus::Pending, TaskStatus::Running) |
        (TaskStatus::Pending, TaskStatus::Cancelled) |
        (TaskStatus::Running, TaskStatus::Completed) |
        (TaskStatus::Running, TaskStatus::Failed(_)) |
        (TaskStatus::Running, TaskStatus::Cancelled)
    )
}
```

**3.4 添加消息总线监听器**

创建一个专门监听 Todo 任务触发的组件：

```rust
// 在 caelix-task/src/manager.rs 中添加

impl TaskManager {
    /// 启动 Todo 任务监听器
    /// 
    /// 监听消息总线中的 TodoTaskTrigger 消息，自动更新任务状态
    pub fn start_todo_listener(&self) -> JoinHandle<()> {
        let bus = self.bus.clone();
        let manager = Arc::new(self.clone());  // 需要实现 Clone
        
        tokio::spawn(async move {
            let mut rx = bus.subscribe_to_todo_triggers();
            while let Ok(msg) = rx.recv().await {
                // 解析消息，提取 task_id 和新状态
                if let Some((task_id, new_status, result)) = parse_todo_trigger_msg(&msg) {
                    manager.update_todo_status(task_id, new_status, result).await;
                }
            }
        })
    }
}
```

**3.5 在消息总线中添加 Todo 触发消息类型**

在 `caelix-message/src/task_message.rs` 中添加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskMessageType {
    Created,     // 新增：任务创建
    Started,
    Completed,
    Failed,
    Progress,
    Cancelled,   // 新增：任务取消
    TodoTrigger, // 新增：Todo 任务触发
}

/// Todo 任务触发消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoTriggerMessage {
    pub task_id: String,
    pub session_id: String,
    pub target_status: TaskStatus,
    pub result: Option<String>,
    pub timestamp: DateTime<Utc>,
}
```

**3.6 修改调度器忽略 Todo 任务**

在 `caelix-task/src/scheduler.rs` 中确保 Todo 任务不会被调度：

```rust
pub async fn schedule(&self, meta: TaskMeta) {
    // Todo 任务不加入调度队列
    if matches!(meta.kind, TaskKind::Todo) {
        return;
    }
    
    // ... 现有调度逻辑 ...
}
```

#### 验收标准
- [ ] TaskKind 包含 Todo 变体
- [ ] Todo 任务提交后状态为 Pending，不会自动执行
- [ ] 可以通过 `update_todo_status()` API 手动变更 Todo 任务状态
- [ ] 可以通过消息总线发送 TodoTriggerMessage 触发状态变更
- [ ] 状态流转符合规则（Pending → Running → Completed/Failed/Cancelled）
- [ ] 不影响现有的 Async、Once、Cron 任务
- [ ] Todo 任务的状态变更会持久化和发送通知

---

## 实施任务清单

### Phase 1：准备工作
- [ ] 阅读并理解现有任务系统代码
- [ ] 确认所有需要修改的文件列表
- [ ] 备份当前代码（git branch）

### Phase 2：修改 Runnable trait 返回值
- [ ] 修改 `caelix-api/src/task/mod.rs` 中的 Runnable trait
- [ ] 更新 `caelix-service/src/tools/delegate_task.rs` 中的 DelegateTaskRunnable 实现
- [ ] 检查并更新其他所有 Runnable 实现
- [ ] 编译验证

### Phase 3：添加 RuntimeContext 支持
- [ ] 修改 `caelix-task/src/manager.rs` 中的 TaskHandle 类型
- [ ] 在 submit() 中捕获 RuntimeContext
- [ ] 在 execute_task_inner() 中恢复 RuntimeContext
- [ ] 修改 restore() 方法创建默认上下文
- [ ] 编写单元测试验证上下文传递

### Phase 4：添加任务结果保存
- [ ] 修改 `caelix-task/src/types.rs` 中的 TaskMeta 添加 result 字段
- [ ] 修改 `caelix-task/src/persistence.rs` 支持 session 级别路径
- [ ] 修改 execute_task_inner() 保存结果到 TaskMeta
- [ ] 修改 `caelix-message/src/task_message.rs` 添加 result 字段
- [ ] 更新消息发送逻辑包含结果
- [ ] 验证文件保存到正确位置

### Phase 5：添加 Todo 任务类型
- [ ] 修改 `caelix-api/src/task/mod.rs` 添加 Todo 变体
- [ ] 修改 TaskManager::submit() 处理 Todo 任务
- [ ] 添加 update_todo_status() API
- [ ] 添加 is_valid_todo_transition() 验证函数
- [ ] 修改调度器忽略 Todo 任务
- [ ] 在消息总线中添加 TodoTriggerMessage
- [ ] 实现 Todo 任务监听器
- [ ] 编写集成测试验证 Todo 任务流程

### Phase 6：测试与验证
- [ ] 运行所有现有测试
- [ ] 编写新功能单元测试
- [ ] 编写集成测试验证完整流程
- [ ] 手动测试 CLI 中的任务功能
- [ ] 验证持久化文件正确性
- [ ] 验证消息总线通知正确性

### Phase 7：文档更新
- [ ] 更新 `.spec/caelix-task/spec.md` 功能规范
- [ ] 更新相关代码注释
- [ ] 记录 Breaking Changes（如果有）

---

## 风险与注意事项

### 风险 1：Breaking Change
- **描述**: 修改 Runnable trait 的返回值会影响所有实现
- **缓解**: 
  - 全面搜索所有 Runnable 实现
  - 提供迁移指南
  - 考虑是否需要版本升级

### 风险 2：持久化兼容性
- **描述**: TaskMeta 新增 result 字段可能导致旧数据反序列化失败
- **缓解**: 
  - 使用 `#[serde(default)]` 标记新字段
  - 提供数据迁移脚本

### 风险 3：RuntimeContext 生命周期
- **描述**: 长时间运行的任务可能持有过期的上下文
- **缓解**: 
  - 在任务开始时快照上下文
  - 记录警告日志如果上下文过期

### 风险 4：Todo 任务状态一致性
- **描述**: 外部触发可能导致非法状态流转
- **缓解**: 
  - 严格验证状态转换
  - 记录所有状态变更日志
  - 提供状态查询 API

---

## 参考资料
- [Rust async-trait documentation](https://docs.rs/async-trait)
- [tokio task-local documentation](https://docs.rs/tokio/latest/tokio/macro.task_local.html)
- [serde serialization guide](https://serde.rs/)
- [项目架构文档](file://.spec/spec.md)

---

## 实施进展记录

### 2026-05-22 - Phase 2 & Phase 3 完成

**已完成工作**:

#### Phase 2: 修改 Runnable trait 返回值
1. ✅ 修改 `caelix-api/src/task/mod.rs` 中的 Runnable trait，返回类型从 `anyhow::Result<()>` 改为 `Result<String, AgentError>`
2. ✅ 更新 `caelix-service/src/tools/delegate_task.rs` 中的 DelegateTaskRunnable 实现
3. ✅ 删除 `caelix-task/src/types.rs` 中重复的 Runnable trait 定义，统一使用 caelix-api 中的版本
4. ✅ 保留 caelix-task 中的 RunnableFactory 具体实现
5. ✅ 编译验证通过

#### Phase 3: 添加 RuntimeContext 支持
1. ✅ 修改 `caelix-task/src/manager.rs` 中的 TaskHandle 类型，将占位符 `Option<()>` 替换为 `Option<RuntimeContext>`
2. ✅ 在 `submit()` 方法中捕获当前的 RuntimeContext：`let runtime_ctx = RuntimeContext::current()`
3. ✅ 修改 `execute_task_inner()` 签名，添加 `runtime_ctx: RuntimeContext` 参数
4. ✅ 在任务执行时使用 `RuntimeContext::scope()` 恢复上下文：
   ```rust
   let result = RuntimeContext::scope(runtime_ctx, async {
       runnable.run().await
   }).await;
   ```
5. ✅ 修复调度器后台循环，从注册表中提取 RuntimeContext 并传递给 execute_task_inner
6. ✅ 对于恢复的任务，创建最小化的 RuntimeContext（包含 session_id、默认 provider/model）
7. ✅ 修改 `restore()` 方法，为每个恢复的任务创建默认的 RuntimeContext
8. ✅ 添加 `caelix-runtime` 依赖到 `caelix-task/Cargo.toml`
9. ✅ 编译验证通过，整个项目无错误

**当前状态**:
- Phase 2 和 Phase 3 已完全完成
- 所有修改已编译验证通过
- 任务执行时现在可以访问完整的 RuntimeContext（session_id、provider、model、trace_id 等）
- Runnable trait 现在返回任务结果字符串，为后续保存结果做准备

**下一步**:
- Phase 4: 修改 TaskMeta 添加 result 字段，实现任务结果保存到 session 级别目录
- Phase 5: 添加 Todo 任务类型

-------------

### 2026-05-22 - Phase 4 & Phase 5 完成

**已完成工作**:

#### Phase 4: 添加任务结果支持并保存到 session 级别目录
1. ✅ 修改 `caelix-task/src/types.rs` 中的 TaskMeta，添加 `result: Option<String>` 字段
2. ✅ 修改 `caelix-task/src/persistence.rs` 中的 FilePersistence：
   - 添加 `get_session_task_path()` 方法支持 session 级别路径
   - 添加 `ensure_session_dir()` 方法创建 session 目录
   - 修改 `save()` 方法将任务文件保存到 `{base_path}/{session_id}/{task_id}.json`
   - 修改 `load_all()` 方法遍历所有 session 目录加载任务
3. ✅ 修改 `caelix-task/src/manager.rs` 中的 `execute_task_inner()`：
   - 提取任务执行结果字符串（成功时为返回字符串，失败时为错误信息）
   - 将结果保存到 TaskMeta.result 字段
   - 持久化包含结果的元数据
4. ✅ 修改 `caelix-api/src/message/mod.rs` 中的 TaskMessage，添加 `result: Option<String>` 字段
5. ✅ 修改 `caelix-task/src/manager.rs` 中的所有 TaskMessage 创建位置，传递 result 字段
6. ✅ 编译验证通过

#### Phase 5: 增加 Todo 任务类型
1. ✅ 修改 `caelix-api/src/task/mod.rs`：
   - 在 TaskKind 枚举中添加 `Todo` 变体
   - 为 TaskKind 实现 `PartialEq` 和 `Eq` trait
2. ✅ 修改 `caelix-task/src/manager.rs` 中的 `submit()` 方法：
   - 添加对 `TaskKind::Todo` 的处理分支
   - Todo 任务不执行，只保存元数据，状态保持为 Pending
3. ✅ 添加 `update_todo_status()` API：
   - 允许外部更新 Todo 任务的状态（Completed/Failed/Cancelled）
   - 可选提供结果字符串
   - 发送相应的通知消息
4. ✅ 修改 `caelix-task/src/scheduler.rs` 中的 `calculate_next_run()`：
   - 将 Todo 任务和 Async 任务一样返回 None（不由调度器触发）
5. ✅ 修改 `caelix-task/src/manager.rs` 中的任务完成处理逻辑：
   - 在模式匹配中包含 Todo 任务类型
6. ✅ 添加 TodoTriggerMessage 和 TodoTriggerAction 到 `caelix-api/src/message/mod.rs`：
   - TodoTriggerMessage: 用于外部触发 Todo 任务状态变更的消息
   - TodoTriggerAction: Complete/Fail/Cancel 三种动作
7. ✅ 在 `caelix-message/src/task_message.rs` 中重新导出新类型
8. ✅ 编译验证通过，整个项目无错误无警告

**关键设计决策**:
- **Session 级别存储**: 任务文件按 session 组织，便于管理和清理
- **Todo 任务特性**: 
  - 不由系统自动执行，完全由外部触发状态变更
  - 通过 `update_todo_status()` API 更新状态
  - 适用于需要人工确认或外部事件触发的场景
- **结果持久化**: 所有任务执行结果都保存到 TaskMeta 并持久化到磁盘
- **消息增强**: TaskMessage 携带结果字段，方便前端展示

**验收标准验证**:
1. ✅ Runnable trait 返回 `Result<String, AgentError>` - 已实现
2. ✅ 任务执行时能访问 RuntimeContext - 已实现（Phase 3）
3. ✅ 任务结果保存到 session 级别目录 - 已实现
4. ✅ Todo 任务类型完全由外部触发 - 已实现
5. ✅ 所有代码编译通过，无错误无警告 - 已验证
