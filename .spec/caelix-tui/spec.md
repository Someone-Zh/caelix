# TUI 界面规范

## 功能概述

TUI（Terminal User Interface）是 Caelix 的终端图形用户界面，使用 Ratatui 框架构建。提供分屏显示、消息历史、任务列表、实时日志等功能，相比 CLI 提供更友好的视觉体验和更丰富的交互方式。

## 核心能力

### 1. 界面布局

**主界面结构**:
```
┌─────────────────────────────────────────────┐
│  Header: Session Info | Agent | Model       │
├──────────────────┬──────────────────────────┤
│                  │                          │
│  Message History │  Current Response        │
│  (Scrollable)    │  (Streaming)             │
│                  │                          │
│                  │                          │
├──────────────────┴──────────────────────────┤
│  Input Area                                 │
│  > _                                        │
├─────────────────────────────────────────────┤
│  Status Bar: Tasks | Notifications          │
└─────────────────────────────────────────────┘
```

**布局组件**:
- **Header**: 显示会话 ID、当前 Agent、模型信息
- **Message History**: 可滚动的消息历史面板
- **Current Response**: 实时流式显示当前 Agent 响应
- **Input Area**: 用户输入区域
- **Status Bar**: 显示任务状态、通知等信息

### 2. 视图系统

**视图类型**:
```rust
pub enum View {
    Chat,           // 聊天主视图
    TaskList,       // 任务列表视图
    SessionList,    // 会话列表视图
    Settings,       // 设置视图
    Help,           // 帮助视图
}
```

**视图切换**:
```rust
impl AppState {
    pub fn switch_view(&mut self, view: View) {
        self.current_view = view;
        self.refresh_view();
    }
}
```

### 3. 状态管理

**AppState 结构**:
```rust
pub struct AppState {
    pub current_view: View,
    pub session_id: String,
    pub agent_name: String,
    pub model_name: String,
    pub messages: Vec<AgentMessage>,
    pub current_response: String,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub scroll_offset: usize,
    pub tasks: Vec<TaskMeta>,
    pub notifications: Vec<NotificationMessage>,
    pub is_processing: bool,
}
```

**状态更新**:
```rust
impl AppState {
    pub fn add_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
        self.auto_scroll();
    }
    
    pub fn update_current_response(&mut self, chunk: &str) {
        self.current_response.push_str(chunk);
    }
    
    pub fn clear_current_response(&mut self) {
        self.current_response.clear();
    }
}
```

### 4. 事件处理

**支持的事件**:
- 键盘输入（字符、方向键、功能键）
- 鼠标事件（点击、滚动）
- 终端resize事件

**事件循环**:
```rust
use crossterm::event::{self, Event, KeyCode, KeyEvent};

pub async fn run_event_loop(
    state: &mut AppState,
    api: Arc<CaelixApi>,
) -> Result<(), TuiError> {
    loop {
        // 渲染界面
        render(state)?;
        
        // 等待事件
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    handle_key_event(state, key_event, &api).await?;
                },
                Event::Resize(width, height) => {
                    state.terminal_size = (width, height);
                },
                _ => {}
            }
        }
        
        // 检查是否需要退出
        if state.should_exit {
            break;
        }
    }
    
    Ok(())
}
```

**键盘事件处理**:
```rust
async fn handle_key_event(
    state: &mut AppState,
    key_event: KeyEvent,
    api: &Arc<CaelixApi>,
) -> Result<(), TuiError> {
    match key_event.code {
        KeyCode::Char(c) => {
            state.input_buffer.push(c);
            state.cursor_position += 1;
        },
        KeyCode::Backspace => {
            if state.cursor_position > 0 {
                state.input_buffer.pop();
                state.cursor_position -= 1;
            }
        },
        KeyCode::Enter => {
            if !state.input_buffer.is_empty() {
                send_message(state, api).await?;
            }
        },
        KeyCode::Up => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1);
        },
        KeyCode::Down => {
            state.scroll_offset += 1;
        },
        KeyCode::Esc => {
            state.should_exit = true;
        },
        _ => {}
    }
    
    Ok(())
}
```

### 5. 视图渲染

**主渲染函数**:
```rust
use ratatui::{Terminal, Frame};
use ratatui::widgets::{Block, Borders, Paragraph, List};

pub fn render(state: &AppState, terminal: &mut Terminal<impl Backend>) -> Result<(), TuiError> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Main content
                Constraint::Length(3),  // Input
                Constraint::Length(2),  // Status bar
            ])
            .split(f.size());
        
        // 渲染各个部分
        render_header(f, chunks[0], state);
        render_main_content(f, chunks[1], state);
        render_input(f, chunks[2], state);
        render_status_bar(f, chunks[3], state);
    })?;
    
    Ok(())
}
```

**消息历史渲染**:
```rust
fn render_message_history(f: &mut Frame, area: Rect, state: &AppState) {
    let messages: Vec<ListItem> = state.messages
        .iter()
        .skip(state.scroll_offset)
        .map(|msg| {
            let content = format!("{}: {}", 
                match msg.r#type {
                    AgentMessageType::User => "👤 You",
                    AgentMessageType::Assistant => "🤖 Agent",
                    _ => "ℹ️ Info",
                },
                msg.content
            );
            ListItem::new(content)
        })
        .collect();
    
    let list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Messages"));
    
    f.render_widget(list, area);
}
```

**流式响应渲染**:
```rust
fn render_current_response(f: &mut Frame, area: Rect, state: &AppState) {
    let paragraph = Paragraph::new(state.current_response.as_str())
        .block(Block::default().borders(Borders::ALL).title("Response"))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}
```

### 6. 命令模式

**TUI 命令**:
```rust
pub enum TuiCommand {
    SendMessage(String),
    SwitchAgent(String),
    SwitchModel(String),
    ShowTasks,
    ShowSessions,
    ClearScreen,
    Exit,
}
```

**命令执行**:
```rust
async fn execute_command(
    state: &mut AppState,
    command: TuiCommand,
    api: &Arc<CaelixApi>,
) -> Result<(), TuiError> {
    match command {
        TuiCommand::SendMessage(msg) => {
            send_message_to_agent(state, msg, api).await?;
        },
        TuiCommand::SwitchAgent(agent) => {
            state.agent_name = agent;
            show_notification(state, format!("Switched to agent: {}", agent));
        },
        TuiCommand::ShowTasks => {
            state.tasks = api.list_tasks(Some(&state.session_id)).await?;
            state.switch_view(View::TaskList);
        },
        TuiCommand::Exit => {
            state.should_exit = true;
        },
        _ => {}
    }
    
    Ok(())
}
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **Runner** | `caelix-tui/src/runner.rs` | TUI 主循环运行器 |
| **State** | `caelix-tui/src/state.rs` | 应用状态管理 |
| **Views** | `caelix-tui/src/views.rs` | 视图渲染逻辑 |
| **Commands** | `caelix-tui/src/commands.rs` | 命令处理 |
| **Events** | `caelix-tui/src/events.rs` | 事件处理 |

### Runner 实现

```rust
use ratatui::{Terminal, backend::CrosstermBackend};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};

pub async fn run_tui(api: Arc<CaelixApi>) -> Result<(), TuiError> {
    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // 创建应用状态
    let mut state = AppState::new(api.clone());
    
    // 运行事件循环
    let result = run_event_loop(&mut state, api).await;
    
    // 清理终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    result
}
```

### 状态管理

```rust
impl AppState {
    pub fn new(api: Arc<CaelixApi>) -> Self {
        Self {
            current_view: View::Chat,
            session_id: api.create_session_sync(),
            agent_name: "planner_agent".to_string(),
            model_name: "gpt-4".to_string(),
            messages: Vec::new(),
            current_response: String::new(),
            input_buffer: String::new(),
            cursor_position: 0,
            scroll_offset: 0,
            tasks: Vec::new(),
            notifications: Vec::new(),
            is_processing: false,
            should_exit: false,
            terminal_size: (80, 24),
        }
    }
    
    pub fn auto_scroll(&mut self) {
        // 自动滚动到底部
        self.scroll_offset = self.messages.len().saturating_sub(20);
    }
}
```

## 用户体验优化

### 1. 语法高亮

**代码块高亮**:
```rust
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn highlight_code(code: &str, language: &str) -> String {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    
    // 实现语法高亮逻辑
    // ...
    
    highlighted_text
}
```

### 2. 进度指示器

**加载动画**:
```rust
fn render_loading_indicator(f: &mut Frame, area: Rect, state: &AppState) {
    if state.is_processing {
        let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let index = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() / 100) as usize % spinner.len();
        
        let text = format!(" {} Processing...", spinner[index]);
        let paragraph = Paragraph::new(text);
        f.render_widget(paragraph, area);
    }
}
```

### 3. 快捷键提示

**帮助面板**:
```rust
fn render_help_panel(f: &mut Frame, area: Rect) {
    let help_text = vec![
        "Ctrl+C: Exit",
        "Enter: Send message",
        "Up/Down: Scroll history",
        "Ctrl+L: Clear screen",
        "/tasks: Show tasks",
        "/agents: Switch agent",
    ];
    
    let paragraph = Paragraph::new(help_text.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Help"));
    
    f.render_widget(paragraph, area);
}
```

### 4. 颜色主题

**自定义主题**:
```rust
#[derive(Clone)]
pub struct ColorTheme {
    pub background: Color,
    pub foreground: Color,
    pub primary: Color,
    pub secondary: Color,
    pub error: Color,
    pub success: Color,
}

impl ColorTheme {
    pub fn dark() -> Self {
        Self {
            background: Color::Rgb(30, 30, 30),
            foreground: Color::White,
            primary: Color::Blue,
            secondary: Color::Yellow,
            error: Color::Red,
            success: Color::Green,
        }
    }
}
```

## 扩展指南

### 添加新视图

**步骤**:

1. **定义视图枚举**
```rust
pub enum View {
    // ...
    CustomView,
}
```

2. **实现渲染函数**
```rust
fn render_custom_view(f: &mut Frame, area: Rect, state: &AppState) {
    let paragraph = Paragraph::new("Custom View Content")
        .block(Block::default().borders(Borders::ALL).title("Custom"));
    
    f.render_widget(paragraph, area);
}
```

3. **在主渲染函数中集成**
```rust
fn render_main_content(f: &mut Frame, area: Rect, state: &AppState) {
    match state.current_view {
        View::Chat => render_chat_view(f, area, state),
        View::CustomView => render_custom_view(f, area, state),
        // ...
    }
}
```

### 自定义主题

```rust
// 在配置文件中定义主题
let theme = load_theme_from_config()?;
state.set_theme(theme);
```

## 性能优化

### 1. 增量渲染

**只渲染变化部分**:
```rust
if state.current_response_changed {
    render_current_response(f, area, state);
    state.current_response_changed = false;
}
```

### 2. 缓冲优化

**减少刷新频率**:
```rust
// 限制最大 FPS
const MAX_FPS: u64 = 30;
let frame_duration = Duration::from_millis(1000 / MAX_FPS);
```

### 3. 内存管理

**限制消息历史大小**:
```rust
const MAX_MESSAGES: usize = 1000;

if state.messages.len() > MAX_MESSAGES {
    state.messages.drain(..state.messages.len() - MAX_MESSAGES);
}
```

## 测试策略

### 单元测试

```rust
#[test]
fn test_state_management() {
    let mut state = AppState::new(mock_api());
    
    state.add_message(create_test_message());
    assert_eq!(state.messages.len(), 1);
}
```

### 集成测试

- 完整用户交互流程测试
- 视图切换测试
- 事件处理测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
