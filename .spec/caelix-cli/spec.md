# CLI 界面规范

## 功能概述

CLI（Command Line Interface）是 Caelix 的默认交互界面，提供基于命令行的用户交互体验。支持会话管理、Agent 切换、模型选择、任务查询等功能，采用流式输出实时显示 Agent 响应。

## 核心能力

### 1. 命令行参数

**启动参数**:
```bash
caelix [options]                    # 默认启动 CLI
caelix cli [options]                # 显式启动 CLI
caelix http [port]                  # 启动 HTTP 服务器
caelix tui                          # 启动 TUI 界面
```

**CLI 选项**:
```bash
-s, --session <ID>     # 指定会话 ID
-a, --agent <NAME>     # 指定使用的 Agent
-p, --provider <NAME>  # 指定 LLM 提供商
-m, --model <NAME>     # 指定模型
-h, --help             # 显示帮助信息
```

**示例**:
```bash
# 使用默认配置启动
caelix

# 指定会话和 Agent
caelix --session sess_123 --agent planner_agent

# 指定提供商和模型
caelix --provider openai --model gpt-4
```

### 2. 交互式对话

**主循环流程**:
```
初始化 → 显示欢迎信息 → 等待用户输入
                              ↓
                       解析输入内容
                              ↓
                      是否为命令？(/xxx)
                        ↓         ↓
                       Yes       No
                        ↓         ↓
                   执行命令   作为消息发送给 Agent
                        ↓         ↓
                   显示结果   流式显示 Agent 响应
                        ↓         ↓
                   继续循环 ←─────┘
```

**对话示例**:
```
🔧 初始化 Caelix 上下文...
✅ 上下文初始化完成
💻 启动 CLI 后端...

========================================
  Caelix AI Assistant
  Session: sess_abc123
  Agent: planner_agent
========================================

请输入您的问题 (输入 /help 查看命令):

> 帮我分析一下这个项目的架构

[Agent 开始思考...]

这个项目采用了分层架构设计...

[流式输出持续显示]

> /help

可用命令:
  /help          - 显示帮助信息
  /session       - 查看当前会话
  /agent         - 切换 Agent
  /model         - 切换模型
  /tasks         - 查看任务列表
  /clear         - 清屏
  /exit          - 退出程序
```

### 3. 内置命令

**命令列表**:

| 命令 | 描述 | 示例 |
|------|------|------|
| `/help` | 显示帮助信息 | `/help` |
| `/session` | 查看/创建会话 | `/session new` |
| `/agent` | 列出或切换 Agent | `/agent list`, `/agent planner_agent` |
| `/model` | 列出或切换模型 | `/model list`, `/model gpt-4` |
| `/provider` | 列出或切换提供商 | `/provider list`, `/provider openai` |
| `/tasks` | 查看任务列表 | `/tasks`, `/tasks --session sess_123` |
| `/messages` | 查看消息历史 | `/messages --last 10` |
| `/clear` | 清屏 | `/clear` |
| `/exit` | 退出程序 | `/exit` |

**命令实现**:
```rust
pub struct CommandHandler {
    api: Arc<CaelixApi>,
}

impl CommandHandler {
    pub async fn handle_command(&self, command: &str) -> Result<String, CliError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.first().unwrap_or(&"");
        
        match *cmd {
            "/help" => self.show_help(),
            "/session" => self.handle_session(&parts[1..]),
            "/agent" => self.handle_agent(&parts[1..]).await,
            "/model" => self.handle_model(&parts[1..]).await,
            "/tasks" => self.handle_tasks(&parts[1..]).await,
            "/clear" => Ok("\x1b[2J\x1b[H".to_string()), // ANSI 清屏
            "/exit" => std::process::exit(0),
            _ => Err(CliError::UnknownCommand(cmd.to_string())),
        }
    }
    
    async fn handle_agent(&self, args: &[&str]) -> Result<String, CliError> {
        if args.is_empty() || args[0] == "list" {
            let agents = self.api.list_agents().await;
            Ok(format!("可用 Agents:\n{}", 
                agents.iter().map(|a| format!("  - {}", a)).collect::<Vec<_>>().join("\n")
            ))
        } else {
            let agent_name = args[0];
            // 切换 Agent 逻辑
            Ok(format!("已切换到 Agent: {}", agent_name))
        }
    }
}
```

### 4. 流式输出

**实时显示 Agent 响应**:
```rust
pub async fn chat_with_streaming(
    api: &Arc<CaelixApi>,
    session_id: &str,
    agent_name: &str,
    message: &str,
) -> Result<(), CliError> {
    let request = ChatRequest {
        session_id: session_id.to_string(),
        agent_name: Some(agent_name.to_string()),
        message: message.to_string(),
    };
    
    let mut stream = api.chat_stream(request).await?;
    
    print!("\n> ");
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                match chunk {
                    AgentOutputChunk::Content { content } => {
                        print!("{}", content);
                        std::io::stdout().flush()?;
                    },
                    AgentOutputChunk::ToolCall { name, .. } => {
                        println!("\n[调用工具: {}]", name);
                    },
                    AgentOutputChunk::Finish { .. } => {
                        println!("\n");
                    },
                    _ => {}
                }
            },
            Err(e) => {
                eprintln!("\n错误: {:?}", e);
                break;
            }
        }
    }
    
    Ok(())
}
```

### 5. 输入处理

**读取用户输入**:
```rust
use std::io::{self, Write};

pub fn read_user_input() -> Result<String, IoError> {
    print!("\n> ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(input.trim().to_string())
}
```

**输入验证**:
```rust
fn validate_input(input: &str) -> Result<(), CliError> {
    if input.is_empty() {
        return Err(CliError::EmptyInput);
    }
    
    if input.len() > MAX_INPUT_LENGTH {
        return Err(CliError::InputTooLong);
    }
    
    Ok(())
}
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **Runner** | `caelix-cli/src/runner.rs` | CLI 主循环运行器 |
| **Commands** | `caelix-cli/src/commands.rs` | 命令处理器 |
| **InputHandler** | `caelix-cli/src/input_handler.rs` | 输入处理器 |

### Runner 实现

```rust
pub async fn run_cli(api: Arc<CaelixApi>) -> Result<(), CliError> {
    // 1. 创建会话
    let session_id = api.create_session().await;
    println!("✅ 创建会话: {}", session_id);
    
    // 2. 获取默认 Agent
    let agents = api.list_agents().await;
    let default_agent = agents.first()
        .ok_or_else(|| CliError::NoAgentsAvailable)?;
    
    println!("🤖 使用 Agent: {}", default_agent);
    
    // 3. 创建命令处理器
    let handler = CommandHandler::new(api.clone());
    
    // 4. 主循环
    loop {
        // 读取输入
        let input = read_user_input()?;
        
        if input.is_empty() {
            continue;
        }
        
        // 判断是否为命令
        if input.starts_with('/') {
            // 执行命令
            match handler.handle_command(&input).await {
                Ok(output) => println!("{}", output),
                Err(e) => eprintln!("❌ 错误: {:?}", e),
            }
        } else {
            // 作为消息发送给 Agent
            match chat_with_streaming(&api, &session_id, default_agent, &input).await {
                Ok(_) => {},
                Err(e) => eprintln!("❌ 错误: {:?}", e),
            }
        }
    }
}
```

### 命令处理器

```rust
pub struct CommandHandler {
    api: Arc<CaelixApi>,
    current_agent: String,
    current_model: String,
}

impl CommandHandler {
    pub fn new(api: Arc<CaelixApi>) -> Self {
        Self {
            api,
            current_agent: "planner_agent".to_string(),
            current_model: "gpt-4".to_string(),
        }
    }
    
    pub async fn handle_command(&self, command: &str) -> Result<String, CliError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        
        match parts.first() {
            Some(&"/help") => self.show_help(),
            Some(&"/agent") => self.handle_agent(&parts[1..]).await,
            Some(&"/model") => self.handle_model(&parts[1..]).await,
            Some(&"/tasks") => self.handle_tasks(&parts[1..]).await,
            _ => Err(CliError::UnknownCommand(command.to_string())),
        }
    }
}
```

## 用户体验优化

### 1. 欢迎信息

```rust
fn print_welcome_message(session_id: &str, agent: &str) {
    println!("\n{}", "=".repeat(40));
    println!("  Caelix AI Assistant");
    println!("  Session: {}", session_id);
    println!("  Agent: {}", agent);
    println!("{}", "=".repeat(40));
    println!("\n请输入您的问题 (输入 /help 查看命令):\n");
}
```

### 2. 进度提示

```rust
// Agent 开始执行时显示提示
println!("\n[Agent 开始思考...]");

// 工具调用时显示
println!("[调用工具: {}]", tool_name);

// 任务执行时显示
println!("[执行任务: {}]", task_description);
```

### 3. 错误提示

```rust
match result {
    Ok(_) => {},
    Err(ApiError::AgentNotFound(name)) => {
        eprintln!("❌ Agent 不存在: {}", name);
        eprintln!("💡 使用 /agent list 查看可用的 Agent");
    },
    Err(ApiError::ProviderError(msg)) => {
        eprintln!("❌ LLM 提供商错误: {}", msg);
        eprintln!("💡 检查 API Key 和网络连接");
    },
    Err(e) => {
        eprintln!("❌ 发生错误: {:?}", e);
    }
}
```

### 4. 颜色输出（可选）

```rust
use colored::*;

println!("{}", "✅ 成功".green());
println!("{}", "❌ 错误".red());
println!("{}", "💡 提示".yellow());
println!("{}", "🔧 初始化".blue());
```

## 扩展指南

### 添加新命令

**步骤**:

1. **在 CommandHandler 中添加处理方法**
```rust
impl CommandHandler {
    async fn handle_custom_command(&self, args: &[&str]) -> Result<String, CliError> {
        // 实现命令逻辑
        Ok("命令执行结果".to_string())
    }
}
```

2. **在 handle_command 中注册**
```rust
match parts.first() {
    // ...
    Some(&"/custom") => self.handle_custom_command(&parts[1..]).await,
    _ => Err(CliError::UnknownCommand(command.to_string())),
}
```

3. **更新帮助信息**
```rust
fn show_help(&self) -> Result<String, CliError> {
    Ok(r#"
可用命令:
  /help          - 显示帮助信息
  /custom        - 自定义命令
  ...
"#.to_string())
}
```

## 测试策略

### 单元测试

```rust
#[tokio::test]
async fn test_command_handler() {
    let api = create_mock_api();
    let handler = CommandHandler::new(api);
    
    let result = handler.handle_command("/help").await;
    assert!(result.is_ok());
}
```

### 集成测试

- 完整对话流程测试
- 命令执行测试
- 流式输出测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
