# 项目结构规范

## 结构

```
caelix/                              # Workspace 根目录
├── .spec/                           # 项目规范文档
│   ├── rules.md                     # 开发规范规则
│   └── spec.md                      # 项目结构规范（本文件）
│
├── caelix-api/                      # 【核心定义层】所有 trait、类型、错误定义
│   ├── src/
│   │   ├── lib.rs                   # 模块入口，重新导出所有公共 API
│   │   ├── agent/                   # Agent 相关定义
│   │   │   └── mod.rs               # AgentSpec, AgentOutputChunk
│   │   ├── tool/                    # Tool 相关定义
│   │   │   └── mod.rs               # Tool trait, ToolDefinition, ToolCall
│   │   ├── provider/                # LLM Provider 相关定义
│   │   │   └── mod.rs               # LlmProvider trait, ChatMessage, ProviderConfig
│   │   ├── message/                 # 消息类型定义
│   │   │   └── mod.rs               # AgentMessage, NotificationMessage, TaskMessage
│   │   ├── task/                    # 任务相关定义
│   │   │   └── mod.rs               # TaskMeta, TaskStatus, Runnable trait
│   │   ├── context/                 # 运行时上下文接口
│   │   │   └── mod.rs               # RuntimeContext trait, AgentRunManagerTrait
│   │   ├── cancel.rs                # CancellationToken 取消令牌
│   │   ├── hooks/                   # Hook 系统接口
│   │   │   └── mod.rs               # Hook trait, HookRegistry
│   │   ├── commands/                # 命令系统接口
│   │   │   └── mod.rs               # Command, CommandType
│   │   ├── error.rs                 # 错误类型定义 (AgentError, ApiError)
│   │   └── utils.rs                 # 通用工具函数 (ID 生成器)
│   └── Cargo.toml
│   依赖: 无内部依赖（最底层）
│
├── caelix-llm/                      # 【LLM 提供者实现】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   └── openai.rs                # OpenAI Provider 实现
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-tools/                    # 【基础工具实现】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口，导出所有工具
│   │   ├── file_edit.rs             # DiffEditTool - 文件差异编辑
│   │   ├── tree.rs                  # DirectoryTreeTool - 目录树浏览
│   │   ├── file_search.rs           # SmartSearchTool - 智能文件搜索
│   │   └── file_read.rs             # ReadFileTool - 文件读取
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-message/                  # 【消息总线系统】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── bus.rs                   # MessageBus - 消息总线实现
│   │   ├── manager.rs               # SessionManager - 会话管理器
│   │   ├── storage.rs               # FileStorage - 文件存储
│   │   ├── agent_message.rs         # Agent 消息处理（兼容性）
│   │   ├── notification_message.rs  # 通知消息处理（兼容性）
│   │   ├── task_message.rs          # 任务消息处理（兼容性）
│   │   └── types.rs                 # 内部类型定义
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-task/                     # 【任务队列系统】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── manager.rs               # TaskManager - 任务管理器
│   │   ├── persistence.rs           # FilePersistence - 任务持久化
│   │   ├── scheduler.rs             # TaskScheduler - 任务调度器
│   │   └── types.rs                 # 任务类型和 Runnable 实现
│   └── Cargo.toml
│   依赖: caelix-api, caelix-message
│
├── caelix-runtime/                  # 【运行时层】Hook 系统 + RuntimeContext 实现
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── context/                 # 运行时上下文实现
│   │   │   ├── mod.rs
│   │   │   └── runtime_context.rs   # RuntimeContext 具体实现
│   │   ├── agent_run_manager.rs     # AgentRunManager 运行中 Agent 管理
│   │   ├── hooks/                   # Hook 系统实现
│   │   │   ├── mod.rs               # HookRegistry
│   │   │   ├── skill_hook.rs        # 技能加载 Hook
│   │   │   ├── message_bus_hook.rs  # 消息总线 Hook
│   │   │   ├── tool_result_check_hook.rs  # 工具结果检查 Hook
│   │   │   └── loader.rs            # Hook 加载器
│   │   ├── commands/                # 命令系统实现
│   │   │   ├── mod.rs
│   │   │   └── command_types.rs     # 命令类型定义
│   │   └── id_generator.rs          # ID 生成器实现
│   └── Cargo.toml
│   依赖: caelix-api, caelix-message, caelix-task
│
├── caelix-agent/                    # 【Agent 引擎】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── loop_runner.rs           # Agent 循环执行器
│   │   ├── loop_agent.rs            # LoopAgent 流式执行器
│   │   ├── agent_runner.rs          # Agent 运行器（消息总线集成）
│   │   ├── tool_executor.rs         # 工具执行器
│   │   └── converter.rs             # 消息转换器
│   └── Cargo.toml
│   依赖: caelix-api, caelix-llm, caelix-tools, caelix-runtime
│
├── caelix-config/                   # 【配置中心】Manager + 配置加载
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── managers/                # 资源管理器
│   │   │   ├── mod.rs               # 统一导出
│   │   │   ├── agent.rs             # AgentManager - Agent 管理
│   │   │   ├── tool.rs              # ToolManager - 工具管理
│   │   │   ├── provider.rs          # ProviderManager - Provider 管理
│   │   │   ├── skill.rs             # SkillManager - 技能管理
│   │   │   └── command.rs           # CommandManager - 命令管理
│   │   ├── provider_loader.rs       # Provider 配置加载器
│   │   ├── tools_loader.rs          # Tools 配置加载器
│   │   ├── agents_loader.rs         # Agents 配置加载器
│   │   ├── skills_loader.rs         # Skills 配置加载器
│   │   └── commands_loader.rs       # Commands 配置加载器
│   └── Cargo.toml
│   依赖: caelix-api, caelix-llm, caelix-tools, caelix-agent, caelix-runtime, caelix-message, caelix-task
│
├── caelix-service/                  # 【服务层】API 实现
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── api_trait.rs             # CaelixApi trait 定义
│   │   ├── api_impl.rs              # CaelixApiImpl 实现
│   │   ├── types.rs                 # 服务层类型 (ChatRequest, SessionSummary 等)
│   │   └── context.rs               # CaelixContext - 全局上下文
│   └── Cargo.toml
│   依赖: caelix-api, caelix-config
│
├── caelix-cli/                      # 【CLI 后端】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── runner.rs                # CLI 运行器
│   │   ├── commands.rs              # CLI 命令处理
│   │   └── input_handler.rs         # 输入处理器
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service
│
├── caelix-http/                     # 【HTTP 后端】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── server.rs                # HTTP 服务器启动
│   │   └── handlers.rs              # HTTP 请求处理器
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service, axum, tower
│
├── caelix-tui/                      # 【TUI 后端】
│   ├── src/
│   │   ├── lib.rs                   # 模块入口
│   │   ├── runner.rs                # TUI 运行器
│   │   ├── state.rs                 # TUI 状态管理
│   │   ├── views.rs                 # 视图渲染
│   │   ├── commands.rs              # TUI 命令处理
│   │   └── events.rs                # 事件处理
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service, ratatui, crossterm
│
└── caelix-bin/                      # 【主程序入口】
    ├── src/
    │   └── main.rs                  # 程序入口，根据 features 启动不同后端
    └── Cargo.toml
    依赖: caelix-config, caelix-service, caelix-cli, caelix-http(optional), caelix-tui(optional)
```

## 模块职责表

| 模块 | 职责 |
|------|------|
| **caelix-api** | 定义所有公共接口、trait、类型、错误。作为整个系统的契约层，不包含任何实现逻辑。所有其他包都依赖此包。 |
| **caelix-llm** | 实现 LLM Provider 接口，目前支持 OpenAI。负责与 LLM API 通信，处理流式响应。可扩展支持其他 LLM 提供商。 |
| **caelix-tools** | 实现基础工具（文件编辑、搜索、读取、目录浏览）。所有工具实现 `Tool` trait，可被 Agent 调用。无系统内部依赖。 |
| **caelix-message** | 实现消息总线系统，包括 MessageBus、SessionManager、FileStorage。负责会话管理、消息持久化、发布订阅机制。 |
| **caelix-task** | 实现任务队列系统，包括 TaskManager、FilePersistence、TaskScheduler。负责任务创建、调度、持久化、定时执行。 |
| **caelix-runtime** | 实现运行时功能，包括 RuntimeContext、HookRegistry、ID 生成器。提供 Agent 执行时的上下文环境和扩展点。 |
| **caelix-agent** | 实现 Agent 执行引擎，包括循环运行器、工具执行器、消息转换器。负责执行 Agent 的推理和行动循环。 |
| **caelix-config** | 实现配置中心和资源管理器。从配置文件动态加载 Agent、Tool、Provider、Skill、Command，并通过 Manager 统一管理。 |
| **caelix-service** | 实现统一的 API 接口（CaelixApi trait），提供会话管理、聊天、任务查询等服务。是业务逻辑的核心实现层。 |
| **caelix-cli** | 实现命令行交互界面。处理用户输入、显示输出、执行 CLI 命令。是默认的交互方式。 |
| **caelix-http** | 实现 HTTP REST API 服务器。将 CaelixApi 暴露为 HTTP 端点，支持远程调用。可选 feature。 |
| **caelix-tui** | 实现终端用户图形界面。使用 Ratatui 构建交互式 TUI，提供更友好的视觉体验。可选 feature。 |
| **caelix-bin** | 主程序入口。解析命令行参数，初始化上下文，根据 features 和参数启动相应的后端（CLI/HTTP/TUI）。 |

## 查找位置表格

| 描述 | 位置 | 备注 |
|------|------|------|
| **服务入口点** | `caelix-bin/src/main.rs` | 主函数入口，根据参数启动不同后端 |
| **API 接口定义** | `caelix-service/src/api_trait.rs` | CaelixApi trait 定义所有对外接口 |
| **API 实现** | `caelix-service/src/api_impl.rs` | CaelixApiImpl 实现具体业务逻辑 |
| **全局上下文** | `caelix-service/src/context.rs` | CaelixContext 包含所有管理器和服务 |
| **配置文件位置** | `~/.caelix/` 或 `$CAELIX_HOME` | 可通过环境变量自定义路径 |
| **Agent 配置** | `$CAELIX_HOME/agents/*.agent` 或 `conf/agents/` | YAML frontmatter + Markdown 格式 |
| **Provider 配置** | `$CAELIX_HOME/providers/*.yaml` | YAML 格式的 LLM 提供商配置 |
| **Skill 配置** | `$CAELIX_HOME/skills/*.skill` | 技能定义文件 |
| **Command 配置** | `$CAELIX_HOME/commands/*.cmd` | 自定义命令定义 |
| **会话数据存储** | `$CAELIX_HOME/sessions/{session_id}/` | 每个会话一个目录，包含消息历史 |
| **任务数据存储** | `$CAELIX_HOME/tasks/` | 任务持久化文件 |
| **Agent 执行引擎** | `caelix-agent/src/loop_runner.rs` | Agent 推理-行动循环实现 |
| **工具执行器** | `caelix-agent/src/tool_executor.rs` | 工具调用和执行逻辑 |
| **消息总线** | `caelix-message/src/bus.rs` | MessageBus 实现发布订阅 |
| **会话管理器** | `caelix-message/src/manager.rs` | SessionManager 管理会话生命周期 |
| **文件存储** | `caelix-message/src/storage.rs` | FileStorage 实现消息持久化 |
| **任务管理器** | `caelix-task/src/manager.rs` | TaskManager 管理任务队列 |
| **任务调度器** | `caelix-task/src/scheduler.rs` | TaskScheduler 定时任务调度 |
| **RuntimeContext** | `caelix-runtime/src/context/runtime_context.rs` | 运行时上下文实现 |
| **AgentRunManager** | `caelix-runtime/src/agent_run_manager.rs` | 运行中 Agent 管理（紧急停止） |
| **CancellationToken** | `caelix-api/src/cancel.rs` | 取消令牌实现 |
| **Hook 注册表** | `caelix-runtime/src/hooks/mod.rs` | HookRegistry 管理所有 Hook |
| **技能 Hook** | `caelix-runtime/src/hooks/skill_hook.rs` | 自动加载和应用技能 |
| **消息总线 Hook** | `caelix-runtime/src/hooks/message_bus_hook.rs` | 自动记录消息到总线 |
| **工具结果检查 Hook** | `caelix-runtime/src/hooks/tool_result_check_hook.rs` | 检查工具执行结果 |
| **Agent 管理器** | `caelix-config/src/managers/agent.rs` | AgentManager 加载和管理 Agent |
| **工具管理器** | `caelix-config/src/managers/tool.rs` | ToolManager 注册和管理工具 |
| **Provider 管理器** | `caelix-config/src/managers/provider.rs` | ProviderManager 管理 LLM 提供商 |
| **技能管理器** | `caelix-config/src/managers/skill.rs` | SkillManager 管理技能 |
| **命令管理器** | `caelix-config/src/managers/command.rs` | CommandManager 管理命令 |
| **HTTP 服务器** | `caelix-http/src/server.rs` | 启动 axum HTTP 服务器 |
| **HTTP 处理器** | `caelix-http/src/handlers.rs` | 处理 HTTP 请求路由 |
| **CLI 运行器** | `caelix-cli/src/runner.rs` | CLI 主循环和命令处理 |
| **TUI 运行器** | `caelix-tui/src/runner.rs` | TUI 主循环和事件处理 |
| **TUI 状态** | `caelix-tui/src/state.rs` | TUI 应用状态管理 |
| **TUI 视图** | `caelix-tui/src/views.rs` | TUI 界面渲染逻辑 |
| **OpenAI Provider** | `caelix-llm/src/openai.rs` | OpenAI API 集成实现 |
| **DiffEditTool** | `caelix-tools/src/file_edit.rs` | 文件差异编辑工具 |
| **DirectoryTreeTool** | `caelix-tools/src/tree.rs` | 目录树浏览工具 |
| **SmartSearchTool** | `caelix-tools/src/file_search.rs` | 智能文件搜索工具 |
| **ReadFileTool** | `caelix-tools/src/file_read.rs` | 文件读取工具 |
| **DelegateTaskTool** | `caelix-task/src/tools/delegate_task.rs` | 任务委派工具 |
| **错误定义** | `caelix-api/src/error.rs` | AgentError 和 ApiError 定义 |
| **ID 生成器** | `caelix-runtime/src/id_generator.rs` | Snowflake ID 生成器 |
| **工具函数** | `caelix-api/src/utils.rs` | Session/Request/Span/Trace ID 生成 |

## 数据流向图

### 1. 用户请求流程（以 CLI 为例）

```
用户输入 → CLI Runner → CaelixApiImpl → AgentManager → AgentSpec
                                    ↓
                              RuntimeContext
                                    ↓
                              Agent Loop Runner
                                    ↓
                          ┌─────────┴─────────┐
                          ↓                   ↓
                   LlmProvider (OpenAI)   Tool Executor
                          ↓                   ↓
                   ChatResponseChunk    ToolResult
                          ↓                   ↓
                    MessageBus ←─────────────┘
                          ↓
                   SessionManager → FileStorage (持久化)
                          ↓
                    CLI Output (流式显示)
```

**详细步骤**:
1. 用户在 CLI 中输入问题或命令
2. CLI Runner 接收输入，调用 `CaelixApiImpl::chat_stream()`
3. CaelixApiImpl 创建 RuntimeContext，设置 session_id、request_id、span_id
4. 通过 AgentManager 获取指定的 AgentSpec
5. Agent Loop Runner 开始执行推理-行动循环：
   - 构建消息列表（system prompt + 历史消息 + 用户输入）
   - 调用 LlmProvider 进行流式聊天
   - 接收 ChatResponseChunk，转换为 AgentOutputChunk
   - 如果检测到工具调用，通过 ToolExecutor 执行工具
   - 将工具结果返回给 LLM，继续下一轮对话
6. 所有消息通过 MessageBus 广播
7. SessionManager 接收消息并持久化到 FileStorage
8. CLI Runner 实时接收流式输出并显示给用户

### 2. 任务委派流程

```
Planner Agent → delegate_task 工具 → TaskManager
                                         ↓
                                  创建子任务 (Runnable)
                                         ↓
                                  TaskScheduler 调度
                                         ↓
                                  执行子任务 Agent
                                         ↓
                                  任务结果持久化
                                         ↓
                                  通知父任务完成
                                         ↓
                            Planner Agent 收到结果继续规划
```

**详细步骤**:
1. Planner Agent 分析任务，决定需要委派子任务
2. 调用 `delegate_task` 工具，指定子任务 Agent 和描述
3. TaskManager 创建新的 TaskMeta，生成 task_id
4. TaskScheduler 将任务加入队列（同步或异步）
5. 如果是同步任务，立即启动子任务 Agent 执行
6. 子任务 Agent 执行完成后，更新任务状态为 Completed
7. TaskManager 将结果写入任务文件，并通过 MessageBus 发送通知
8. Planner Agent 通过工具查询任务状态或直接接收通知
9. Planner Agent 根据子任务结果继续规划或总结

### 3. Hook 执行流程

```
Agent 执行前 → HookRegistry::before_agent()
                    ↓
            遍历所有注册的 Hook
                    ↓
            SkillHook: 加载技能文档
                    ↓
            MessageBusHook: 记录 Agent 开始
                    ↓
            自定义 Hook...
                    ↓
            Agent 执行
                    ↓
Agent 执行后 → HookRegistry::after_agent()
                    ↓
            ToolResultCheckHook: 检查结果
                    ↓
            MessageBusHook: 记录 Agent 结束
                    ↓
            自定义 Hook...
```

**详细步骤**:
1. Agent 执行前，LoopRunner 调用 `HookRegistry::before_agent()`
2. HookRegistry 遍历所有注册的 Hook，按优先级执行
3. SkillHook 根据 Agent 名称和 group 加载对应的技能文档
4. MessageBusHook 发送 AgentStart 消息到消息总线
5. Agent 开始执行推理-行动循环
6. Agent 执行完成后，调用 `HookRegistry::after_agent()`
7. ToolResultCheckHook 检查最后一次工具调用的结果
8. MessageBusHook 发送 AgentEnd 消息
9. 其他自定义 Hook 执行清理或后处理逻辑

### 4. 消息持久化流程

```
AgentOutputChunk → MessageBus::publish()
                        ↓
                所有订阅者接收消息
                        ↓
                SessionManager::on_message()
                        ↓
                判断消息类型：
                - Chunk: 暂存到 agent_buffers
                - Msg: 直接持久化
                        ↓
                FileStorage::append_agent_message()
                        ↓
                写入 sessions/{session_id}/messages.jsonl
                        ↓
                定期 Flush: agent_buffers → 持久化
```

**详细步骤**:
1. Agent 产生输出块（AgentOutputChunk）
2. 转换为 AgentMessage，通过 MessageBus 发布
3. SessionManager 订阅消息总线，接收所有消息
4. 对于 Chunk 类型的消息，暂存到内存中的 agent_buffers
5. 对于 Msg 类型的消息，立即调用 FileStorage 持久化
6. FileStorage 将消息追加写入 JSONL 文件
7. 在特定时机（会话结束、Ctrl+C、定期），Flush 所有 Chunk 消息
8. 确保所有消息最终都持久化到磁盘

### 5. 紧急停止流程

```
用户触发 stop_agent(session_id)
            ↓
    CaelixApiImpl::stop_agent()
            ↓
    AgentRunManager::stop_agent()
            ↓
    ┌───────────────────────────┐
    │ 1. 从 DashMap 中移除记录    │
    │ 2. CancellationToken.cancel()
    │ 3. AbortHandle.abort()     │
    └───────────────────────────┘
            ↓
    ┌───────────────────────────┐
    │ LLM 流中的 select! 检测到  │
    │ cancellation_token 取消    │
    └───────────────────────────┘
            ↓
    产出 AgentOutputChunk::Stopped
            ↓
    agent_runner 发送 ChunkEnd
            ↓
    提前退出，不持久化部分内容
```

**详细步骤**:
1. 外部调用 `CaelixApi::stop_agent(session_id)` 触发紧急停止
2. CaelixApiImpl 从 CaelixContext 获取 AgentRunManager
3. AgentRunManager 通过 session_id 从 DashMap 查找运行中的 Agent
4. 同时触发两种取消机制：
   - `CancellationToken.cancel()` - 优雅取消，让流检测到后主动退出
   - `AbortHandle.abort()` - 强制中止任务（双保险）
5. LLM 流中 `tokio::select!` 检测到取消信号，立即断开 HTTP 连接
6. 产出 `AgentOutputChunk::Stopped { reason: "cancelled_by_user" }`
7. agent_runner 收到 Stopped 后发送 ChunkEnd 消息，提前结束
8. 已收到的部分 LLM 内容**完全抛弃，不持久化**
9. 任务结束后自动从 AgentRunManager 注销

**关键特性**:
- 按 session 隔离：一个 session 同时只有一个运行中的 Agent
- 双保险取消：CancellationToken（优雅） + AbortHandle（强制）
- 即时中断：LLM HTTP 连接立即断开，不等待响应完成
- 内容丢弃：部分接收的内容不持久化，保持数据一致性
- 资源清理：任务结束自动注销，无内存泄漏

## 依赖关系

### 层级依赖图

```
Level 0: caelix-api (无内部依赖，所有包的基础)
    ↑
    ├─ Level 1: caelix-llm, caelix-tools, caelix-message
    │   ↑
    │   └─ Level 2: caelix-task (依赖 message)
    │       ↑
    │       └─ Level 3: caelix-runtime (依赖 message + task)
    │           ↑
    │           └─ Level 4: caelix-agent (依赖 llm + tools + runtime)
    │               ↑
    │               └─ Level 5: caelix-config (依赖几乎所有上层包)
    │                   ↑
    │                   └─ Level 6: caelix-service (依赖 config)
    │                       ↑
    │                       ├─ Level 7: caelix-cli, caelix-http, caelix-tui
    │                       │   ↑
    │                       │   └─ Level 8: caelix-bin (聚合所有 backend)
```

### 关键依赖说明

1. **caelix-api**: 
   - 无任何内部依赖
   - 所有其他包都直接或间接依赖它
   - 定义了系统的核心契约

2. **caelix-message ↔ caelix-task**:
   - task 依赖 message（任务需要发送消息）
   - message 不依赖 task（避免循环依赖）

3. **caelix-runtime**:
   - 依赖 message 和 task
   - 提供 Hook 系统和 RuntimeContext
   - 是 Agent 执行的运行时环境

4. **caelix-agent**:
   - 依赖 llm、tools、runtime
   - 不包含配置管理，保持纯粹的执行引擎

5. **caelix-config**:
   - 依赖几乎所有上层包
   - 是唯一可以访问所有管理器的包
   - 负责初始化和装配所有组件

6. **caelix-service**:
   - 仅依赖 config
   - 通过 config 间接访问所有功能
   - 实现统一的 API 接口

7. **表现层 (cli/http/tui)**:
   - 仅依赖 service
   - 不直接访问底层实现
   - 通过 API 接口交互

### 禁止的依赖方向

- ❌ 下层包不能依赖上层包（如 api 不能依赖 service）
- ❌ 同层包之间尽量避免依赖（如 llm 和 tools 互不依赖）
- ❌ 表现层不能绕过 service 直接访问 config 或 runtime
- ❌ 避免循环依赖，使用 trait 抽象打破依赖环

## 项目下功能

| 功能 | 位置 | 描述 |
|------|------|------|
| **Agent 系统** | [caelix-agent/spec.md](file://caelix-agent/spec.md) | 多 Agent 协作架构，支持 planner、executor、collector 等角色。Agent 通过配置文件定义，动态加载。支持 Hook 机制扩展行为。包含推理-行动循环、工具调用、流式输出等核心能力。 |
| **工具系统** | [caelix-tools/spec.md](file://caelix-tools/spec.md) | 可扩展的工具框架，内置文件编辑、搜索、读取、目录浏览等工具。通过实现 Tool trait 添加新工具。支持工具参数校验、执行结果处理、错误恢复。Agent 可通过工具与环境交互。 |
| **消息总线** | [caelix-message/spec.md](file://caelix-message/spec.md) | 发布订阅模式的消息系统，支持多类型消息（Agent、Notification、Task）。SessionManager 管理会话生命周期，FileStorage 实现消息持久化。支持流式消息广播和实时订阅。 |
| **任务调度** | [caelix-task/spec.md](file://caelix-task/spec.md) | 异步任务队列系统，支持任务创建、调度、执行、持久化。TaskManager 管理任务状态，TaskScheduler 支持定时任务（cron）。支持任务委派（delegate_task），实现 Agent 间协作。核心特性：任务返回值 Result<String, AgentError>、RuntimeContext 完整传递、任务结果保存到 session 级别目录、Todo 待办任务类型（外部触发状态变更）。 |
| **Hook 系统** | [caelix-runtime/spec.md](file://caelix-runtime/spec.md) | 运行时扩展机制，支持在 Agent 执行前后注入自定义逻辑。内置技能加载、消息记录、工具结果检查等 Hook。通过 HookRegistry 统一管理，支持优先级和条件匹配。 |
| **配置管理** | [caelix-config/spec.md](file://caelix-config/spec.md) | 动态配置加载和资源管理系统。从文件系统加载 Agent、Provider、Tool、Skill、Command 配置。通过 Manager 模式统一管理各类资源，支持热重载。 |
| **CLI 界面** | [caelix-cli/spec.md](file://caelix-cli/spec.md) | 命令行交互界面，默认启动模式。支持会话管理、Agent 切换、模型选择、任务查询等命令。流式输出实时显示，支持历史记录和命令补全。 |
| **HTTP API** | [caelix-http/spec.md](file://caelix-http/spec.md) | RESTful API 服务，可选 feature。将 CaelixApi 暴露为 HTTP 端点，支持远程调用。使用 axum 框架，支持 CORS、错误处理、流式响应。适合集成到其他系统。 |
| **TUI 界面** | [caelix-tui/spec.md](file://caelix-tui/spec.md) | 终端用户图形界面，可选 feature。使用 Ratatui 构建交互式 UI，提供更友好的视觉体验。支持分屏显示、消息历史、任务列表、实时日志等。 |
| **LLM Provider** | [caelix-llm/spec.md](file://caelix-llm/spec.md) | LLM 提供商抽象层，目前支持 OpenAI。通过 LlmProvider trait 定义接口，可扩展支持其他提供商（Anthropic、Google 等）。支持流式响应、工具调用、多模型切换。 |
| **会话管理** | [caelix-message/spec.md](file://caelix-message/spec.md#会话管理) | 会话生命周期管理，包括创建、查询、删除、持久化。每个会话有唯一 ID，隔离消息历史。支持会话摘要、消息检索、跨会话引用。 |
| **技能系统** | [caelix-runtime/spec.md](file://caelix-runtime/spec.md#技能系统) | 技能文档自动加载和应用机制。Skill 是 Markdown 格式的指令集，Hook 系统在 Agent 执行前自动注入相关技能。支持按 Agent 名称和 group 匹配技能。 |
| **ID 生成** | [caelix-runtime/spec.md](file://caelix-runtime/spec.md#id-生成) | 分布式 ID 生成器，使用 Snowflake 算法。生成 session_id、request_id、span_id、task_id、trace_id。保证全局唯一性和时间有序性，支持分布式追踪。 |
| **紧急停止** | [caelix-runtime/spec.md](file://caelix-runtime/spec.md#紧急停止) | 支持立即中断当前 LLM 调用并退出 Agent。使用 CancellationToken + AbortHandle 双保险机制，按 session 隔离。LLM HTTP 连接立即断开，已接收的部分内容完全抛弃不持久化。通过 AgentRunManager 统一管理运行中的 Agent。 |

---

**最后更新**: 2026-07-02  
**维护者**: Caelix 开发团队
