# Caelix(天庭)

一个模块化的 Rust AI Agent 框架，采用 Workspace 多包架构设计。

## 项目架构

Caelix 采用分层架构设计，将功能拆分为 13 个独立的 crate，实现清晰的依赖关系和高度的模块化。

### 架构图

```
caelix/                          # Workspace 根目录
├── Cargo.toml                   # workspace 声明
├── README.md
│
├── caelix-api/                  # 【核心定义层】所有 trait、类型、错误定义
│   ├── src/
│   │   ├── lib.rs
│   │   ├── agent/               # Agent trait + AgentSpec/AgentOutputChunk
│   │   ├── tool/                # Tool trait + ToolDefinition/ToolResult/ToolCall
│   │   ├── provider/            # LlmProvider trait + ChatMessage/LlmConfig
│   │   ├── message/             # AgentMessage/NotificationMessage/TaskMessage
│   │   ├── task/                # TaskMeta/TaskStatus/Runnable trait
│   │   ├── context/             # RuntimeContext trait（接口定义）
│   │   ├── hooks/               # Hook 系统接口定义
│   │   ├── error.rs             # AgentError/ApiError 等
│   │   └── utils.rs             # 通用工具函数（ID 生成器等）
│   └── Cargo.toml
│
├── caelix-llm/                  # 【LLM 提供者实现】
│   ├── src/
│   │   ├── lib.rs
│   │   └── openai.rs            # OpenAI 提供者实现
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-tools/                # 【基础工具实现】无系统内部依赖的工具
│   ├── src/
│   │   ├── lib.rs
│   │   ├── file_edit.rs         # DiffEditTool
│   │   ├── tree.rs              # DirectoryTreeTool
│   │   ├── file_search.rs       # SmartSearchTool
│   │   └── file_read.rs         # ReadFileTool
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-message/              # 【消息总线系统】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── bus.rs               # MessageBus
│   │   ├── manager.rs           # SessionManager
│   │   ├── storage.rs           # FileStorage
│   │   ├── agent_message.rs
│   │   ├── notification_message.rs
│   │   ├── task_message.rs
│   │   └── types.rs
│   └── Cargo.toml
│   依赖: caelix-api
│
├── caelix-task/                 # 【任务队列系统】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── manager.rs           # TaskManager
│   │   ├── persistence.rs       # FilePersistence
│   │   ├── scheduler.rs
│   │   ├── types.rs             # Runnable 实现
│   │   └── tools/
│   │       └── delegate_task.rs # DelegateTaskTool
│   └── Cargo.toml
│   依赖: caelix-api, caelix-message
│
├── caelix-runtime/              # 【运行时层】Hook系统 + RuntimeContext实现
│   ├── src/
│   │   ├── lib.rs
│   │   ├── context/
│   │   │   ├── mod.rs
│   │   │   └── runtime_context.rs  # RuntimeContext 实现
│   │   ├── hooks/
│   │   │   ├── mod.rs           # HookRegistry
│   │   │   ├── skill_hook.rs
│   │   │   ├── message_bus_hook.rs
│   │   │   ├── tool_result_check_hook.rs
│   │   │   └── loader.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   └── command_types.rs
│   │   └── id_generator.rs
│   └── Cargo.toml
│   依赖: caelix-api, caelix-message, caelix-task
│
├── caelix-agent/                # 【Agent 引擎】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── executor.rs          # execute_agent_with_messaging
│   │   ├── loop_runner.rs
│   │   ├── tool_executor.rs
│   │   └── converter.rs
│   └── Cargo.toml
│   依赖: caelix-api, caelix-llm, caelix-tools, caelix-runtime
│
├── caelix-config/               # 【配置中心】Manager + 配置加载
│   ├── src/
│   │   ├── lib.rs
│   │   ├── context.rs           # CaelixContext
│   │   ├── provider_loader.rs
│   │   ├── tools_loader.rs
│   │   ├── agents_loader.rs
│   │   ├── skills_loader.rs
│   │   ├── commands_loader.rs
│   │   └── managers/
│   │       ├── mod.rs
│   │       ├── agent.rs         # AgentManager
│   │       ├── tool.rs          # ToolManager
│   │       ├── provider.rs      # ProviderManager
│   │       ├── skill.rs         # SkillManager
│   │       └── command.rs       # CommandManager
│   └── Cargo.toml
│   依赖: caelix-api, caelix-llm, caelix-tools, caelix-agent, caelix-runtime, caelix-message, caelix-task
│
├── caelix-service/              # 【服务层】API 实现
│   ├── src/
│   │   ├── lib.rs
│   │   ├── api_trait.rs         # CaelixApi trait
│   │   ├── api_impl.rs          # CaelixApiImpl 实现
│   │   └── types.rs             # ChatRequest/ApiError/SessionSummary 等
│   └── Cargo.toml
│   依赖: caelix-api, caelix-config
│
├── caelix-cli/                  # 【CLI 后端】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── runner.rs
│   │   ├── commands.rs
│   │   └── input_handler.rs
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service
│
├── caelix-http/                 # 【HTTP 后端】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── server.rs
│   │   └── handlers.rs
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service, axum, tower
│
├── caelix-tui/                  # 【TUI 后端】
│   ├── src/
│   │   ├── lib.rs
│   │   ├── runner.rs
│   │   ├── state.rs
│   │   ├── views.rs
│   │   ├── commands.rs
│   │   └── events.rs
│   └── Cargo.toml
│   依赖: caelix-api, caelix-service, ratatui, crossterm
│
└── caelix-bin/                  # 【主程序入口】
    ├── src/
    │   └── main.rs
    └── Cargo.toml
    依赖: caelix-config, caelix-service, caelix-cli, caelix-http(optional), caelix-tui(optional)
```

### 依赖关系图

```
caelix-api (最底层，无内部依赖)
    ↑
    ├── caelix-llm
    ├── caelix-tools
    ├── caelix-message
    └── caelix-task (依赖 message)
        ↑
    caelix-runtime (依赖 message + task)
        ↑
    caelix-agent (依赖 llm + tools + runtime)
        ↑
    caelix-config (依赖几乎所有上层包)
        ↑
    caelix-service (依赖 config)
        ↑
    ┌───┬───────┬──────┐
    ↓   ↓       ↓      ↓
caelix-cli caelix-http caelix-tui
    ↑
caelix-bin (聚合所有 backend)
```

## 快速开始

### 编译项目

```bash
# 编译整个 workspace
cargo build --workspace

# 仅编译主程序（默认 CLI 模式）
cargo build -p caelix-bin

# 编译带 HTTP 服务器支持
cargo build -p caelix-bin --features http-server

# 编译带 TUI 支持
cargo build -p caelix-bin --features tui

# 编译全部功能
cargo build -p caelix-bin --features "http-server,tui"
```

### 运行

```bash
# 运行 CLI 模式（默认）
cargo run -p caelix-bin

# 运行 HTTP 服务器
cargo run -p caelix-bin --features http-server -- http

# 运行 TUI 界面
cargo run -p caelix-bin --features tui -- tui
```

### CLI 参数说明

CLI 支持以下参数：

```bash
# 基本用法
caelix cli [OPTIONS]

# 选项:
#   -s, --session <SESSION_ID>  指定会话ID（未提供则自动创建）
#   -a, --agent <AGENT>         指定使用的 agent（未提供则使用第一个可用）
#   -p, --provider <PROVIDER>   指定提供商（未提供则使用默认）
#   -m, --model <MODEL>         指定模型（未提供则使用默认）
#   -c, --content <CONTENT>     快速对话模式：直接指定消息内容，对话结束后退出
```

#### 快速对话模式 (-c)

使用 `-c` 参数可以快速进行一次对话，对话结束后自动退出：

```bash
# 简单问候
cargo run -p caelix-bin -- cli -c "你好"

# 结合指定会话（加载历史对话）
cargo run -p caelix-bin -- cli -s S-7462532379215663104 -c "请总结一下我们之前的对话"

# 结合指定 Agent
cargo run -p caelix-bin -- cli -a planner_agent -c "帮我分析当前项目的依赖结构"

# 组合多个参数
cargo run -p caelix-bin -- cli -s my-session -a code_executor_agent -p bailian -m qwen-plus -c "解释一下这段代码"
```

**注意**：
- 使用 `-c` 参数时，程序会在完成一次对话后自动退出
- 可以与其他参数（`-s`, `-a`, `-p`, `-m`）组合使用
- 如果指定了 `-s`，会加载对应会话的历史对话作为上下文

## 核心特性

### 1. 模块化架构
- **13 个独立 crate**：每个包职责单一，依赖关系清晰
- **自底向上构建**：从核心定义层到应用层，逐层抽象
- **可选后端**：CLI、HTTP、TUI 三种交互方式可按需启用

### 2. Agent 系统
- **多 Agent 协作**：支持 planner、executor、collector 等多种 Agent 角色
- **Hook 机制**：在 Agent 执行前后注入自定义逻辑（技能加载、消息记录等）
- **流式输出**：支持实时流式响应，提升用户体验

### 3. 工具系统
- **内置工具**：文件编辑、目录浏览、智能搜索、文件读取
- **可扩展**：通过实现 `Tool` trait 轻松添加新工具
- **委托任务**：支持 Agent 间任务委派和子任务管理

### 4. 消息总线
- **会话管理**：自动持久化会话历史到文件系统
- **多类型消息**：Agent 消息、通知消息、任务消息
- **广播机制**：支持多订阅者实时接收消息流

### 5. 任务调度
- **异步执行**：基于 tokio 的异步任务系统
- **持久化**：任务状态自动保存到磁盘
- **定时任务**：支持 cron 表达式定时执行

### 6. 配置管理
- **动态加载**：从配置文件动态加载 Agent、工具、技能定义
- **管理器模式**：统一的 Manager 接口管理各类资源
- **嵌入资源**：使用 rust-embed 打包默认配置文件

## 开发指南

### 添加新工具

1. 在 `caelix-tools/src/` 创建新文件
2. 实现 `caelix_api::tool::Tool` trait
3. 在 `caelix-tools/src/lib.rs` 中导出
4. 在配置文件中注册工具

### 添加新 Agent

1. 在 `conf/agents/` 创建 `.agent` 配置文件
2. 定义 Agent 的角色、系统提示词、可用工具
3. 通过 `AgentManager` 动态加载

### 自定义 Hook

1. 实现 `caelix_api::hooks::Hook` trait
2. 在 `caelix-runtime/src/hooks/` 注册 Hook
3. 在 Agent 配置中指定需要应用的 Hook

### 扩展 LLM Provider

1. 在 `caelix-llm/src/` 创建新的 provider 实现
2. 实现 `caelix_api::provider::LlmProvider` trait
3. 在 `caelix-config/src/provider_loader.rs` 中注册

## 技术栈

- **异步运行时**: tokio
- **序列化**: serde, serde_json, serde_yaml
- **HTTP 框架**: axum, tower
- **TUI 框架**: ratatui, crossterm
- **并发容器**: dashmap
- **日志追踪**: tracing, tracing-subscriber
- **错误处理**: thiserror, anyhow
- **UUID 生成**: uuid, snowflaked

## 项目结构说明

### 核心层（Core Layer）
- **caelix-api**: 定义所有公共接口、trait、类型
- **caelix-llm**: LLM 提供者实现（OpenAI 等）
- **caelix-tools**: 基础工具实现

### 运行时层（Runtime Layer）
- **caelix-message**: 消息总线和会话管理
- **caelix-task**: 任务调度和持久化
- **caelix-runtime**: 运行时上下文和 Hook 系统
- **caelix-agent**: Agent 执行引擎

### 服务层（Service Layer）
- **caelix-config**: 配置中心和资源管理器
- **caelix-service**: API 接口实现

### 表现层（Presentation Layer）
- **caelix-cli**: 命令行界面
- **caelix-http**: HTTP REST API
- **caelix-tui**: 终端用户界面
- **caelix-bin**: 主程序入口

## 贡献指南

欢迎提交 Issue 和 Pull Request！在贡献代码时请遵循以下原则：

1. **保持模块化**：新功能应尽量放入独立的 crate
2. **遵循依赖规则**：避免循环依赖，保持自底向上的依赖方向
3. **编写测试**：为核心逻辑添加单元测试
4. **更新文档**：修改 API 时同步更新文档注释

## License

本项目采用 MIT License