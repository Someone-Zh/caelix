# 任务调度系统规范

## 功能概述

任务调度系统是 Caelix 的异步任务管理基础设施，提供任务创建、调度、执行、持久化和定时执行能力。支持任务委派（delegate_task），实现 Agent 间的协作和子任务管理。

## 核心能力

### 1. 任务模型

**TaskMeta**: 任务元数据
```rust
pub struct TaskMeta {
    pub id: String,              // 任务唯一 ID
    pub session_id: String,      // 所属会话
    pub parent_task_id: Option<String>, // 父任务 ID
    pub agent_name: String,      // 执行 Agent
    pub description: String,     // 任务描述
    pub status: TaskStatus,      // 任务状态
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,  // 执行结果
    pub error: Option<String>,   // 错误信息
}

pub enum TaskStatus {
    Pending,     // 等待执行
    Running,     // 执行中
    Completed,   // 已完成
    Failed,      // 失败
    Cancelled,   // 已取消
}
```

**Runnable Trait**: 可执行任务
```rust
#[async_trait]
pub trait Runnable: Send + Sync {
    async fn run(&self, context: &RuntimeContext) -> Result<String, AgentError>;
}
```

### 2. 任务生命周期

```
创建 (Pending) → 调度 → 执行 (Running) → 完成 (Completed/Failed)
     ↑                                              |
     └──────────── 重试/取消 ←─────────────────────┘
```

**状态转换**:
1. **Pending**: 任务创建后初始状态
2. **Running**: 调度器选择任务开始执行
3. **Completed**: 任务成功执行完毕
4. **Failed**: 任务执行失败
5. **Cancelled**: 任务被手动取消

### 3. 任务调度

**TaskScheduler 职责**:
1. 管理任务队列
2. 按优先级调度任务
3. 支持定时任务（cron）
4. 控制并发执行数量
5. 处理任务超时

**调度策略**:
- **FIFO**: 先进先出（默认）
- **Priority**: 按优先级排序
- **Cron**: 定时触发
- **Dependency**: 依赖关系调度

**示例**:
```rust
// 创建定时任务
let task = TaskMeta {
    description: "每日备份".to_string(),
    schedule: Some("0 0 * * *".to_string()), // 每天午夜
    ..Default::default()
};

task_scheduler.schedule(task).await?;
```

### 4. 任务持久化

**FilePersistence 实现**:
```rust
pub struct FilePersistence {
    base_path: PathBuf,
}

impl FilePersistence {
    pub async fn save_task(&self, task: &TaskMeta) -> Result<(), IoError> {
        let file_path = self.base_path.join(format!("{}.json", task.id));
        let content = serde_json::to_string_pretty(task)?;
        tokio::fs::write(&file_path, content).await?;
        Ok(())
    }
    
    pub async fn load_task(&self, task_id: &str) -> Result<Option<TaskMeta>, IoError> {
        let file_path = self.base_path.join(format!("{}.json", task_id));
        if !file_path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&file_path).await?;
        let task = serde_json::from_str(&content)?;
        Ok(Some(task))
    }
}
```

**存储结构**:
```
tasks/
├── task_001.json
├── task_002.json
└── task_003.json
```

### 5. 任务委派（Delegate Task）

**DelegateTaskTool**: Agent 间任务委派工具

**参数**:
```json
{
  "agent": "collector_agent",
  "description": "收集项目依赖信息",
  "sync": true,
  "timeout": 300
}
```

**执行流程**:
```
Planner Agent 调用 delegate_task
          ↓
   TaskManager 创建子任务
          ↓
   TaskScheduler 调度执行
          ↓
   启动 Collector Agent
          ↓
   执行子任务
          ↓
   更新任务状态和结果
          ↓
   返回结果给 Planner Agent
```

**同步 vs 异步**:
- **sync=true**: 阻塞等待子任务完成，直接返回结果
- **sync=false**: 立即返回 task_id，后续通过查询获取结果

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **TaskManager** | `caelix-task/src/manager.rs` | 任务管理器 |
| **TaskScheduler** | `caelix-task/src/scheduler.rs` | 任务调度器 |
| **FilePersistence** | `caelix-task/src/persistence.rs` | 任务持久化 |
| **DelegateTaskTool** | `caelix-task/src/tools/delegate_task.rs` | 任务委派工具 |

### TaskManager 实现

```rust
pub struct TaskManager {
    persistence: Arc<FilePersistence>,
    scheduler: Arc<TaskScheduler>,
    tasks: Arc<DashMap<String, TaskMeta>>,
}

impl TaskManager {
    pub async fn create_task(&self, task: TaskMeta) -> Result<String, ApiError> {
        let task_id = task.id.clone();
        
        // 保存到持久化存储
        self.persistence.save_task(&task).await?;
        
        // 加入内存缓存
        self.tasks.insert(task_id.clone(), task);
        
        // 提交到调度器
        self.scheduler.submit(task_id.clone()).await?;
        
        Ok(task_id)
    }
    
    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskMeta>, ApiError> {
        // 先查内存
        if let Some(task) = self.tasks.get(task_id) {
            return Ok(Some(task.value().clone()));
        }
        
        // 再查持久化
        self.persistence.load_task(task_id).await
    }
    
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        result: Option<String>,
    ) -> Result<(), ApiError> {
        let mut task = self.tasks.get_mut(task_id).ok_or(ApiError::TaskNotFound)?;
        task.status = status;
        task.result = result;
        task.completed_at = Some(Utc::now());
        
        // 持久化更新
        self.persistence.save_task(&task).await?;
        
        Ok(())
    }
}
```

### TaskScheduler 实现

```rust
pub struct TaskScheduler {
    queue: Arc<Mutex<VecDeque<String>>>,
    running_tasks: Arc<DashMap<String, JoinHandle<()>>>,
    max_concurrent: usize,
}

impl TaskScheduler {
    pub async fn submit(&self, task_id: String) -> Result<(), ApiError> {
        self.queue.lock().await.push_back(task_id);
        self.try_schedule().await;
        Ok(())
    }
    
    async fn try_schedule(&self) {
        while self.running_tasks.len() < self.max_concurrent {
            if let Some(task_id) = self.queue.lock().await.pop_front() {
                let handle = tokio::spawn(self.execute_task(task_id.clone()));
                self.running_tasks.insert(task_id, handle);
            } else {
                break;
            }
        }
    }
    
    async fn execute_task(&self, task_id: String) {
        // 执行任务逻辑
        // ...
        
        // 任务完成后清理
        self.running_tasks.remove(&task_id);
        self.try_schedule().await;
    }
}
```

### DelegateTaskTool 实现

```rust
#[derive(Debug)]
pub struct DelegateTaskTool {
    task_manager: Arc<TaskManager>,
}

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }
    
    fn description(&self) -> &str {
        "委派任务给其他 Agent 执行"
    }
    
    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "目标 Agent 名称"
                },
                "description": {
                    "type": "string",
                    "description": "任务描述"
                },
                "sync": {
                    "type": "boolean",
                    "description": "是否同步等待结果"
                }
            },
            "required": ["agent", "description"]
        })
    }
    
    async fn execute(&self, input: JsonValue) -> ToolResult {
        let agent = input["agent"].as_str().unwrap();
        let description = input["description"].as_str().unwrap();
        let sync = input["sync"].as_bool().unwrap_or(false);
        
        // 创建子任务
        let task = TaskMeta {
            agent_name: agent.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            ..Default::default()
        };
        
        let task_id = self.task_manager.create_task(task).await
            .map_err(|e| ToolResult {
                output: String::new(),
                error: Some(e.to_string()),
            })?;
        
        if sync {
            // 等待任务完成
            let result = self.wait_for_task_completion(&task_id).await;
            ToolResult {
                output: result.unwrap_or_default(),
                error: None,
            }
        } else {
            // 异步返回 task_id
            ToolResult {
                output: format!("Task created with ID: {}", task_id),
                error: None,
            }
        }
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
```

## 任务查询

### 查询接口

```rust
// 获取任务列表
let tasks = task_manager.list_tasks(Some(&session_id)).await?;

// 获取单个任务
let task = task_manager.get_task(&task_id).await?;

// 按状态过滤
let pending_tasks = task_manager.list_tasks_by_status(TaskStatus::Pending).await?;

// 获取子任务
let subtasks = task_manager.get_subtasks(&parent_task_id).await?;
```

### API 端点

**HTTP API**:
```
GET /api/tasks?session_id={session_id}
GET /api/tasks/{task_id}
POST /api/tasks/{task_id}/cancel
```

**CLI 命令**:
```bash
caelix task list --session sess_123
caelix task show task_456
caelix task cancel task_456
```

## 定时任务

### Cron 表达式支持

**格式**: `分 时 日 月 周`

**示例**:
- `0 * * * *`: 每小时执行
- `0 0 * * *`: 每天午夜执行
- `*/5 * * * *`: 每 5 分钟执行
- `0 9 * * 1`: 每周一上午 9 点执行

**实现**:
```rust
use cron::Schedule;

let schedule = Schedule::from_str("0 0 * * *").unwrap();
let next_run = schedule.upcoming(Utc).next().unwrap();

// 定期检查并触发任务
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        scheduler.check_and_trigger_scheduled_tasks().await;
    }
});
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `TaskNotFound` | 任务不存在 | 返回错误提示 |
| `TaskExecutionFailed` | 任务执行失败 | 记录错误，标记为 Failed |
| `TaskTimeout` | 任务超时 | 取消任务，标记为 Failed |
| `MaxRetriesExceeded` | 超过最大重试次数 | 标记为 Failed |
| `InvalidCronExpression` | Cron 表达式无效 | 拒绝创建任务 |

### 重试机制

```rust
const MAX_RETRIES: usize = 3;

for attempt in 1..=MAX_RETRIES {
    match execute_task(&task).await {
        Ok(result) => {
            update_task_status(&task_id, TaskStatus::Completed, Some(result)).await?;
            break;
        },
        Err(e) if attempt < MAX_RETRIES => {
            warn!("Task {} failed (attempt {}): {:?}", task_id, attempt, e);
            tokio::time::sleep(Duration::from_secs(attempt as u64 * 5)).await;
        },
        Err(e) => {
            error!("Task {} failed after {} attempts: {:?}", task_id, MAX_RETRIES, e);
            update_task_status(&task_id, TaskStatus::Failed, Some(e.to_string())).await?;
            break;
        }
    }
}
```

## 性能优化

### 1. 并发控制

**限制并发任务数**:
```rust
const MAX_CONCURRENT_TASKS: usize = 10;

if running_tasks.len() >= MAX_CONCURRENT_TASKS {
    // 等待有任务完成
    wait_for_available_slot().await;
}
```

### 2. 资源隔离

**每个任务独立的 RuntimeContext**:
```rust
let task_context = RuntimeContext::new(session_id.clone())
    .with_request_id(generate_request_id())
    .with_span_id(generate_span_id());
```

### 3. 懒加载

**延迟加载任务详情**:
```rust
// 列表查询只返回摘要
let summaries = tasks.iter().map(|t| TaskSummary {
    id: t.id.clone(),
    status: t.status.clone(),
    created_at: t.created_at,
}).collect();

// 详情页才加载完整信息
let full_task = task_manager.get_task(&task_id).await?;
```

## 扩展指南

### 添加自定义任务类型

1. **实现 Runnable trait**
```rust
#[derive(Debug, Clone)]
pub struct MyCustomTask {
    pub param1: String,
    pub param2: i32,
}

#[async_trait]
impl Runnable for MyCustomTask {
    async fn run(&self, context: &RuntimeContext) -> Result<String, AgentError> {
        // 实现任务逻辑
        Ok(format!("Result: {} {}", self.param1, self.param2))
    }
}
```

2. **注册任务处理器**
```rust
task_scheduler.register_handler("my_custom_task", Box::new(MyCustomTaskHandler));
```

### 自定义持久化后端

**实现 Persistence trait**:
```rust
#[async_trait]
pub trait Persistence: Send + Sync {
    async fn save_task(&self, task: &TaskMeta) -> Result<(), PersistenceError>;
    async fn load_task(&self, task_id: &str) -> Result<Option<TaskMeta>, PersistenceError>;
    async fn list_tasks(&self) -> Result<Vec<TaskMeta>, PersistenceError>;
}

// 实现数据库持久化
pub struct DatabasePersistence {
    pool: PgPool,
}

#[async_trait]
impl Persistence for DatabasePersistence {
    // ...
}
```

## 测试策略

### 单元测试

```rust
#[tokio::test]
async fn test_task_creation() {
    let manager = create_test_task_manager();
    let task = TaskMeta {
        description: "Test task".to_string(),
        ..Default::default()
    };
    
    let task_id = manager.create_task(task).await.unwrap();
    assert!(!task_id.is_empty());
}

#[tokio::test]
async fn test_delegate_task_sync() {
    let tool = DelegateTaskTool::new(test_task_manager());
    let input = serde_json::json!({
        "agent": "collector_agent",
        "description": "Test delegation",
        "sync": true
    });
    
    let result = tool.execute(input).await;
    assert!(result.error.is_none());
}
```

### 集成测试

- 完整任务生命周期测试
- 并发任务执行测试
- 定时任务触发测试
- 任务持久化和恢复测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
