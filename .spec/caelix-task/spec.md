# 任务调度系统功能规范

## 正在处理的需求目录

## 涉及的模型文件 
|描述|位置|
|---|---|
|任务类型定义（TaskId, TaskKind, TaskStatus, Runnable trait）|caelix-api/src/task/mod.rs|
|任务元数据（TaskMeta）、工厂（RunnableFactory）|caelix-task/src/types.rs|
|任务管理器（TaskManager）|caelix-task/src/manager.rs|
|任务持久化（TaskPersistence, FilePersistence）|caelix-task/src/persistence.rs|
|任务调度器（TaskScheduler）|caelix-task/src/scheduler.rs|
|运行时上下文（RuntimeContext）|caelix-runtime/src/context/runtime_context.rs|
|任务消息类型（TaskMessage, TodoTriggerMessage）|caelix-message/src/task_message.rs|

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
  - `Todo`: 待办任务，完全由外部触发状态变更，不自动执行
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

**异常场景**:
- 任务执行失败时，状态更新为 `Failed(error_message)`，result 字段包含错误信息
- 任务被取消时，立即 abort tokio task

**校验逻辑**:
- TaskId 必须唯一
- Cron 表达式必须符合 cron crate 的语法
- Todo 任务状态流转必须符合规则：Pending → Running → Completed/Failed/Cancelled

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
  - **新增**: result 字段存储任务执行结果（成功时为输出字符串，失败时为错误信息）
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
  - `submit()`: 提交新任务，根据 TaskKind 决定立即执行、调度或仅记录（Todo）
  - `cancel()`: 取消任务，abort tokio task 并更新状态
  - `get_status()`: 查询任务状态
  - `wait()`: 等待任务完成（自旋等待）
  - `list_tasks()`: 列出任务（支持按 session 过滤）
  - `update_progress()`: 更新任务进度并发送通知
  - **新增**: `update_todo_status()`: 外部触发 Todo 任务状态变更
  - **新增**: `start_todo_listener()`: 启动 Todo 任务监听器
  - `restore()`: 从持久化存储恢复任务（启动时调用）

- **内部机制**:
  - 使用 DashMap 存储任务句柄（TaskHandle）
  - TaskHandle 包含：TaskMeta, oneshot::Sender<Result<String, AgentError>>, RuntimeContext, JoinHandle
  - 后台调度器循环检查待执行任务
  - 任务执行完成后发送状态更新消息到 MessageBus
  - **新增**: 任务执行时在 RuntimeContext 作用域内运行，确保上下文传播
  - **新增**: Todo 任务监听器接收消息总线触发，自动更新状态

**关键改进**:
- RuntimeContext 完整传递：任务执行时可以访问 session_id、provider、model、span_id、trace_id
- 任务结果保存：Result<String, AgentError> 保存到 TaskMeta.result 字段
- 持久化路径优化：任务文件保存到 `sessions/{session_id}/tasks/{task_id}.json`
- Todo 任务支持：完全由外部触发，不自动执行

**功能实际文件位置**: `caelix-task/src/manager.rs`

---

### 4. 任务持久化（caelix-task/src/persistence.rs）

**业务逻辑和功能介绍**:
- **TaskPersistence trait**: 持久化接口
  - `save(meta)`: 保存任务元数据（包含 result）
  - `delete(task_id)`: 删除任务文件
  - `load_all()`: 加载所有任务（用于恢复）

- **FilePersistence 实现**:
  - **主存储位置**: `sessions/{session_id}/tasks/{task_id}.json`（session 级别）
  - **备用存储位置**: `$CAELIX_HOME/tasks/{task_id}.json`（全局，向后兼容）
  - 格式：JSON（serde_json 序列化）
  - 所有任务都持久化，包括 Async、Todo 任务
  - 状态变更时立即更新文件

**异常场景**:
- 文件读写失败时返回 anyhow::Error
- session 目录不存在时自动创建

**功能实际文件位置**: `caelix-task/src/persistence.rs`

---

### 5. 任务调度器（caelix-task/src/scheduler.rs）

**业务逻辑和功能介绍**:
- **TaskScheduler**: 任务调度器
  - 维护优先队列（BinaryHeap），按执行时间排序
  - `schedule(meta)`: 将任务加入调度队列（Todo 任务会被忽略）
  - `next_ready()`: 获取下一个就绪的任务（阻塞等待）
  - `cancel(task_id)`: 从调度队列移除任务
  - `calculate_next_run(kind)`: 计算下次执行时间（针对 Cron 任务）

**关键改进**:
- Todo 任务不会加入调度队列，完全由外部触发

**功能实际文件位置**: `caelix-task/src/scheduler.rs`

---

### 6. 任务消息系统（caelix-message/src/task_message.rs）

**业务逻辑和功能介绍**:
- **TaskMessage**: 任务状态变更消息
  - 包含 task_id, session_id, type, timestamp, content
  - **新增**: result 字段包含任务执行结果
  - 消息类型：Created, Started, Completed, Failed, Progress, Cancelled, TodoTrigger

- **TodoTriggerMessage**: Todo 任务触发消息
  - 包含 task_id, session_id, target_status, result, timestamp
  - 通过消息总线发送，触发 Todo 任务状态变更

- **MessageBus 集成**:
  - `send_task(msg)`: 发送任务消息
  - `send_todo_trigger(msg)`: 发送 Todo 触发消息
  - `subscribe_to_tasks()`: 订阅任务消息
  - `subscribe_to_todo_triggers()`: 订阅 Todo 触发消息

**功能实际文件位置**: `caelix-message/src/task_message.rs`

---

## 已有的子模块
|描述|位置|
|---|---|
|delegate_task 工具|[caelix-service/src/tools/delegate_task.rs](file://caelix-service/src/tools/delegate_task.rs)|
|list_tasks 工具|[caelix-service/src/tools/list_tasks.rs](file://caelix-service/src/tools/list_tasks.rs)|

---

## 业务流程图

### 1. Async 任务执行流程

```
用户/Agent → submit(Async, runnable)
                ↓
        捕获 RuntimeContext
                ↓
        创建 TaskMeta (status=Running)
                ↓
        存入 registry + 持久化
                ↓
        Spawn tokio task
                ↓
    ┌───────────────────────────┐
    ↓                           ↓
RuntimeContext::scope      执行 runnable.run()
                ↓                   ↓
          返回 Result<String, AgentError>
                ↓
        更新 TaskMeta.status + result
                ↓
        持久化到 sessions/{session_id}/tasks/{task_id}.json
                ↓
        发送 TaskMessage (Completed/Failed)
                ↓
        通知等待者 (oneshot channel)
```

### 2. Todo 任务状态变更流程

```
外部系统 → 发送 TodoTriggerMessage
                ↓
        MessageBus 广播
                ↓
        TodoListener 接收消息
                ↓
        解析 task_id, target_status, result
                ↓
        调用 update_todo_status()
                ↓
        验证状态流转合法性
                ↓
        更新 TaskMeta.status + result
                ↓
        持久化到 sessions/{session_id}/tasks/{task_id}.json
                ↓
        发送 TaskMessage (Started/Completed/Failed)
```

### 3. 任务恢复流程（启动时）

```
系统启动 → TaskManager::restore()
                ↓
        加载所有任务文件
                ↓
    ┌───────────────────────────┐
    ↓                           ↓
Async/Once/Cron             Todo
    ↓                           ↓
重置为 Scheduled            保持 Pending
    ↓                           ↓
创建默认 RuntimeContext     创建默认 RuntimeContext
    ↓                           ↓
重新加入调度队列            仅注册，不调度
    ↓                           ↓
等待调度器执行              等待外部触发
```

---

## 数据流向图

### 任务数据存储结构

```
$CAELIX_HOME/
├── sessions/
│   ├── {session_id_1}/
│   │   ├── messages.jsonl          # 会话消息历史
│   │   └── tasks/                  # 任务结果目录
│   │       ├── {task_id_1}.json    # 任务 1 的完整元数据和结果
│   │       ├── {task_id_2}.json    # 任务 2 的完整元数据和结果
│   │       └── ...
│   └── {session_id_2}/
│       └── tasks/
│           └── ...
└── tasks/                          # 全局任务目录（向后兼容）
    ├── {task_id_old}.json
    └── ...
```

### TaskMeta JSON 结构示例

```json
{
  "task_id": "T-1234567890-001",
  "session_id": "S-9876543210",
  "span_id": "Span-abc123",
  "tool_call_id": "call_xyz789",
  "task_name": "代码审查任务",
  "kind": "Async",
  "status": "Completed",
  "progress": 1.0,
  "created_at": "2026-05-22T10:00:00Z",
  "updated_at": "2026-05-22T10:05:30Z",
  "task_type_name": "code_review",
  "task_payload": "{\"file_path\": \"src/main.rs\"}",
  "result": "代码审查完成，发现 3 个警告，0 个错误。建议优化错误处理逻辑。"
}
```

---

## 状态流转规则

### Async/Once/Cron 任务
```
Pending → Scheduled → Running → Completed
                            ↘     ↘
                              Failed
                            ↘     ↘
                              Cancelled
```

### Todo 任务
```
Pending → Running → Completed
         ↘     ↘
           Failed
         ↘     ↘
           Cancelled

注意：
- Todo 任务只能由外部触发状态变更
- 不能自动从 Pending 转为 Running
- 必须通过 update_todo_status() 或 TodoTriggerMessage 触发
```

---

## API 使用示例

### 1. 提交 Async 任务

```rust
let task_manager = context.get_task_manager();

let runnable = Box::new(MyCustomTask::new("param1".to_string()));

let task_id = task_manager.submit(
    session_id.clone(),
    span_id.clone(),
    Some(tool_call_id),
    Some("我的任务".to_string()),
    TaskKind::Async,
    runnable,
).await;

// 等待任务完成
if let Some(result) = task_manager.wait(task_id).await {
    match result {
        Ok(_) => println!("任务成功"),
        Err(e) => println!("任务失败: {}", e),
    }
}
```

### 2. 提交 Todo 任务

```rust
let task_manager = context.get_task_manager();

let runnable = Box::new(MyCustomTask::new("param1".to_string()));

let task_id = task_manager.submit(
    session_id.clone(),
    span_id.clone(),
    None,
    Some("待办任务".to_string()),
    TaskKind::Todo,
    runnable,
).await;

// 此时任务状态为 Pending，不会执行

// 稍后通过 API 触发
task_manager.update_todo_status(
    task_id.clone(),
    TaskStatus::Running,
    None,
).await;

// 模拟任务执行...
task_manager.update_todo_status(
    task_id.clone(),
    TaskStatus::Completed,
    Some("任务完成".to_string()),
).await;
```

### 3. 通过消息总线触发 Todo 任务

```rust
let bus = context.get_message_bus();

let trigger_msg = TodoTriggerMessage {
    task_id: task_id.to_string(),
    session_id: session_id.clone(),
    target_status: TaskStatus::Completed,
    result: Some("通过消息触发的完成".to_string()),
    timestamp: Utc::now(),
};

bus.send_todo_trigger(trigger_msg).unwrap();
```

### 4. 查询任务结果

```rust
if let Some(meta) = task_manager.get_status(&task_id).await {
    println!("任务状态: {:?}", meta.status);
    println!("任务结果: {:?}", meta.result);
}
```

---

## 注意事项

### Breaking Changes
1. **Runnable trait 签名变更**:
   - 旧: `async fn run(&self) -> anyhow::Result<()>`
   - 新: `async fn run(&self) -> Result<String, AgentError>`
   - 迁移: 所有实现需要更新返回值，成功时返回结果字符串，失败时返回 AgentError

2. **TaskMeta 新增字段**:
   - 新增 `result: Option<String>`
   - 使用 `#[serde(default)]` 保证向后兼容
   - 旧数据加载时 result 为 None

3. **持久化路径变更**:
   - 新任务保存到 `sessions/{session_id}/tasks/`
   - 旧的全局路径 `$CAELIX_HOME/tasks/` 仍然可用（向后兼容）

### 性能考虑
- RuntimeContext 克隆开销较小（主要是 String 和 PathBuf）
- 任务结果字符串不宜过大，建议限制在 10KB 以内
- Todo 监听器使用独立 tokio task，不影响主流程

### 安全考虑
- 任务结果可能包含敏感信息，注意日志脱敏
- Todo 触发消息需要验证来源，防止未授权状态变更
- 任务文件权限应设置为仅当前用户可读

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
