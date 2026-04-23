# 异步任务流式输出和命令系统实现总结

## 概述

本次实现完成了两个主要功能:
1. **异步任务流式输出**: `chat_stream` 改为异步触发,支持会话切换时持续接收流式内容
2. **命令系统**: 从 `.md` 文件加载提示词和 shell 命令

## 主要改动

### 1. 消息类型扩展 (`src/runtime/message/types.rs`)

在 `MessageMeta` 中添加了两个字段:
- `stream_id: Option<String>` - 流式消息组ID,同一组流式chunk共享此ID
- `is_final: bool` - 是否为流的最后一条消息

这使得可以将多条流式 chunk 消息关联为同一个逻辑流。

### 2. chat_stream 异步化 (`src/api/core.rs`)

**关键改动**:
- `chat_stream` 不再直接返回 agent 执行流
- 生成唯一的 `stream_id` 并在后台任务中执行 agent
- 通过消息总线逐块推送流式内容,每条消息都带上 `stream_id` 和 `is_final` 标记
- 立即返回空流,告知客户端任务已在后台执行

**流程**:
```
用户发送消息 
  → chat_stream 生成 stream_id 
  → 启动后台任务执行 agent
  → 立即返回空流
  → 后台任务通过消息总线推送 chunk
  → TUI 订阅消息总线接收并重组流
```

### 3. SessionManager 扩展 (`src/runtime/message/manager.rs`)

新增两个方法:
- `get_messages_by_stream_id()` - 获取指定 stream_id 的所有消息
- `get_incomplete_streams()` - 获取所有未完成的流式消息组

这些方法用于在切换会话时恢复未完成的流。

### 4. 命令系统实现

#### 4.1 命令类型定义 (`src/enhancement/commands/command_types.rs`)

定义了:
- `CommandType` 枚举: `Prompt` (提示词) 和 `Shell` (shell命令)
- `Command` 结构: 包含 name, description, type, content

#### 4.2 命令管理器 (`src/manager/command.rs`)

`CommandManager` 提供:
- `register()` / `register_batch()` - 注册命令
- `get_all()` - 获取所有命令
- `get_by_name()` - 按名称查询
- `get_by_type()` - 按类型过滤

#### 4.3 命令加载器 (`src/config/commands_loader.rs`)

- 使用 `parse_yaml_markdown_file` 解析 `.md` 文件
- YAML 部分包含: name, description, type (可选,默认为 prompt)
- Markdown 部分作为命令内容
- 递归扫描 `$CAELIX_HOME/commands` 目录下的所有 `.md` 文件

#### 4.4 集成到 CaelixContext (`src/config/context.rs`)

- 添加 `command_manager` 字段
- 新增 `init_commands()` 方法
- 在 `init()` 中调用初始化

### 5. TUI 应用更新 (`src/backends/tui/app.rs`)

#### 5.1 新增字段
- `active_streams: HashMap<String, String>` - 跟踪活跃的流及其累积内容
- `completed_streams: HashSet<String>` - 记录已完成的流

#### 5.2 消息处理逻辑
- 检测消息是否带有 `stream_id`
- 如果是流式消息:
  - 追加内容到 `active_streams[stream_id]`
  - 如果 `is_final=true`,将完整内容作为助手消息添加,清理 active_streams
  - 如果 `is_final=false`,实时更新最后一条助手消息
- 流完成时自动取消加载状态

#### 5.3 发送消息逻辑
- 调用 `chat_stream` 后立即返回(不等待)
- 实际内容通过消息总线异步推送
- 不再手动管理流式接收循环

## 工作流程示例

### 场景1: 正常聊天
1. 用户输入消息并按下 Enter
2. TUI 调用 `chat_stream()`,立即返回
3. 后台任务开始执行 agent
4. 消息总线开始推送 chunk 消息
5. TUI 收到 chunk,实时更新界面
6. 收到 `is_final=true` 的消息,标记完成

### 场景2: 切换会话
1. 用户在 session A 发送消息,任务开始执行
2. 用户切换到 session B
3. session A 的后台任务继续执行,chunk 持续推送到消息总线
4. 用户切换回 session A
5. TUI 从 SessionManager 加载历史消息(包括已完成的 chunk)
6. TUI 订阅消息总线,继续接收新的 chunk
7. 流完成后显示完整内容

### 场景3: 加载命令
1. 应用启动时调用 `init_commands()`
2. 扫描 `$CAELIX_HOME/commands` 目录
3. 解析所有 `.md` 文件
4. 注册到 `CommandManager`
5. 可通过 API 或工具查询和使用命令

## 测试建议

### 1. 验证异步流式输出
```bash
# 启动应用
cargo run --features tui

# 在 TUI 中:
# 1. 发送一条消息
# 2. 观察到 "AI 正在回复..." 状态
# 3. 看到流式内容逐步显示
# 4. 完成后状态变为 "就绪"
```

### 2. 验证会话切换
```bash
# 1. 在 session A 发送长消息(触发长时间执行)
# 2. 快速切换到 session B (按 /tasks 或其他视图)
# 3. 观察 session A 的任务是否在后台继续
# 4. 切换回 session A
# 5. 验证是否能继续看到流式内容更新
```

### 3. 验证命令加载
```bash
# 1. 创建示例命令文件到 ~/.caelix/commands/test.md
# 2. 启动应用
# 3. 检查日志是否显示 "Commands loaded. Total commands: X"
# 4. 通过调试方式验证命令是否正确加载
```

## 注意事项

1. **消息顺序**: 消息总线中的消息按 `seq` 有序处理,TUI 需要确保按顺序重组
2. **内存管理**: `active_streams` 会在流完成后自动清理,避免内存泄漏
3. **错误处理**: 异步任务失败时会通过消息总线发送错误消息
4. **向后兼容**: 现有的同步调用模式已被替换为异步模式
5. **性能考虑**: 大量并发流式消息可能影响性能,后续可考虑批量处理优化

## 已知问题

1. 未使用的导入警告(不影响功能)
2. `chunk_count` 变量未使用(可以移除)
3. 加载状态在流完成时才取消,可能需要超时机制防止卡住

## 后续改进建议

1. 添加流式消息的超时机制
2. 实现命令的执行功能(目前只支持加载)
3. 添加命令的搜索和过滤功能
4. 优化大量流式消息的性能
5. 添加流式进度的可视化指示器
