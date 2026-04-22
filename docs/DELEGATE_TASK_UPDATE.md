# 委派任务工具更新说明

## 概述

已对 `DelegateTaskTool` 进行了重大更新，增加了同步/异步执行模式的支持。

## 主要修改

### 1. 新增参数

在工具的 JSON Schema 中添加了 `sync` 参数：
- **类型**: boolean
- **默认值**: true（同步执行）
- **说明**: 
  - `true`: 同步执行，等待 agent 完成并返回结果
  - `false`: 异步执行，立即返回任务 ID

### 2. 新增字段

`DelegateTaskTool` 结构体新增了 `task_manager` 字段：
```rust
pub struct DelegateTaskTool {
    context: Arc<CaelixContext>,
    message_bus: Option<Arc<MessageBus>>,
    task_manager: Option<Arc<TaskManager>>,  // 新增
}
```

### 3. 执行模式

#### 同步模式 (sync=true)
- 直接执行委派的 agent
- 等待执行完成
- 返回完整的执行结果
- 适用于需要立即获取结果的场景

#### 异步模式 (sync=false)
- 创建 `DelegateTaskRunnable` 包装器
- 将任务提交到 `TaskManager` 的任务队列
- 立即返回任务 ID
- 任务在后台执行
- 执行结果会通过消息总线发送
- 适用于长时间运行的任务

### 4. 新增内部结构

创建了 `DelegateTaskRunnable` 结构来实现 `Runnable` trait：
```rust
struct DelegateTaskRunnable {
    context: Arc<CaelixContext>,
    agent_name: String,
    task_content: String,
    session_id: String,
    span_id: String,
    message_bus: Option<Arc<MessageBus>>,
}
```

该结构负责在异步模式下执行委派任务，并将结果发送到消息总线。

### 5. API 变化

#### 工具调用示例

**同步调用**（默认）：
```json
{
  "agent_name": "code_executor_agent",
  "task_content": "查看当前目录下的文件结构"
}
```

**异步调用**：
```json
{
  "agent_name": "code_executor_agent",
  "task_content": "查看当前目录下的文件结构",
  "sync": false
}
```

#### 返回值

**同步模式**：
```json
{
  "output": "执行结果的完整内容...",
  "error": null
}
```

**异步模式**：
```json
{
  "output": "任务已提交，任务ID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
  "error": null
}
```

### 6. 配置要求

要使用异步模式，需要在创建工具时提供 `TaskManager`：

```rust
let delegate_tool = create_delegate_task_tool(
    context,
    Some(message_bus),      // 可选：用于发送消息
    Some(task_manager),     // 必需：用于异步任务管理
);
```

如果未配置 `TaskManager` 而尝试使用异步模式，将返回错误：
```
"异步执行需要配置 TaskManager"
```

## 文件修改清单

1. **src/base/tool/delegate_task.rs**
   - 添加 `task_manager` 字段
   - 实现 `execute_sync` 方法
   - 实现 `execute_async` 方法
   - 创建 `DelegateTaskRunnable` 结构
   - 更新参数 schema

2. **src/config/tools_loader.rs**
   - 更新 `create_delegate_task_tool` 函数签名
   - 添加 `message_bus` 和 `task_manager` 参数

3. **src/config/context.rs**
   - 更新工具初始化调用

4. **src/config/agents_loader.rs**
   - 为 planner_agent 添加委派任务工具
   - 更新工具创建调用

## 使用建议

1. **短期任务**：使用同步模式（默认），简单直接
2. **长期任务**：使用异步模式，避免阻塞
3. **批量任务**：使用异步模式，可以并行执行多个任务
4. **任务追踪**：异步模式返回的任务 ID 可用于查询任务状态

## 注意事项

- 异步模式需要正确配置 `TaskManager` 和 `MessageBus`
- 异步任务的结果会通过消息总线发送，需要订阅相应的 session
- 任务执行失败时，错误信息也会通过消息总线发送
