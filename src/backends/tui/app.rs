use std::sync::Arc;
use std::time::Instant;
use futures::StreamExt;
use tokio::sync::mpsc;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use crate::api::{CaelixApi, CaelixApiImpl, ChatRequest};
use crate::base::agent::AgentOutputChunk;
use super::events::{EventHandler, TuiEvent};
use super::ui;

/// 对话消息类型
#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Assistant,
    System,
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct Message {
    pub msg_type: MessageType,
    pub content: String,
    pub timestamp: Instant,
}

/// 通知类型
#[derive(Debug, Clone)]
pub enum NotificationType {
    Info,
    Success,
    Error,
    Warning,
}

/// 通知消息（右侧气泡）
#[derive(Debug, Clone)]
pub struct Notification {
    pub notif_type: NotificationType,
    pub message: String,
    pub timestamp: Instant,
}

/// TUI 应用状态
pub struct App {
    pub session_id: Option<String>,
    pub input_buffer: String,
    pub messages: Vec<Message>,  // 对话历史
    pub notifications: Vec<Notification>,  // 通知队列
    pub scroll_offset: u16,
    pub current_provider: String,
    pub current_model: String,
    pub current_agent: String,
    pub available_agents: Vec<String>,  // 可用的 agent 列表
    pub running: bool,
    pub is_loading: bool,  // 是否正在加载 AI 响应
    pub loading_start_time: Option<Instant>,  // 加载开始时间
    pub has_started_chat: bool,  // 是否已经开始对话（用于切换视图）
    pub status_message: String,  // 状态栏消息
    // 用于异步任务通信的通道
    pub message_tx: Option<mpsc::Sender<AppMessage>>,
    pub message_rx: Option<mpsc::Receiver<AppMessage>>,
}

/// 应用内部消息（用于异步任务与主循环通信）
#[derive(Debug, Clone)]
pub enum AppMessage {
    AddMessage(Message),
    AddNotification(Notification),
    SetLoading(bool),
    UpdateStatus(String),
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            session_id: None,
            input_buffer: String::new(),
            messages: Vec::new(),
            notifications: Vec::new(),
            scroll_offset: 0,
            current_provider: "bailian".to_string(),
            current_model: "qwen-max".to_string(),
            current_agent: "default".to_string(),
            available_agents: vec!["default".to_string()],
            running: true,
            is_loading: false,
            loading_start_time: None,
            has_started_chat: false,
            status_message: "就绪".to_string(),
            message_tx: Some(tx),
            message_rx: Some(rx),
        }
    }

    /// 添加对话消息
    pub fn add_message(&mut self, msg: Message) {
        self.messages.push(msg);
        self.scroll_offset = self.messages.len() as u16;
    }

    /// 添加用户消息
    pub fn add_user_message(&mut self, content: &str) {
        self.add_message(Message {
            msg_type: MessageType::User,
            content: content.to_string(),
            timestamp: Instant::now(),
        });
    }

    /// 添加助手消息
    pub fn add_assistant_message(&mut self, content: &str) {
        self.add_message(Message {
            msg_type: MessageType::Assistant,
            content: content.to_string(),
            timestamp: Instant::now(),
        });
    }

    /// 添加通知
    pub fn add_notification(&mut self, notif: Notification) {
        self.notifications.push(notif);
        // 只保留最近 10 个通知
        if self.notifications.len() > 10 {
            self.notifications.remove(0);
        }
    }

    /// 处理内部消息
    pub fn handle_app_message(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::AddMessage(message) => {
                self.add_message(message);
            }
            AppMessage::AddNotification(notification) => {
                self.add_notification(notification);
            }
            AppMessage::SetLoading(loading) => {
                self.is_loading = loading;
                if loading {
                    self.loading_start_time = Some(Instant::now());
                } else {
                    self.loading_start_time = None;
                }
            }
            AppMessage::UpdateStatus(status) => {
                self.status_message = status;
            }
        }
    }

    /// 切换到下一个 agent
    pub fn next_agent(&mut self) {
        if self.available_agents.len() > 1 {
            let current_idx = self.available_agents.iter()
                .position(|a| a == &self.current_agent)
                .unwrap_or(0);
            let next_idx = (current_idx + 1) % self.available_agents.len();
            self.current_agent = self.available_agents[next_idx].clone();
        }
    }
}

/// 运行 TUI 应用
pub async fn run_tui(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建应用状态
    let mut app = App::new();
    
    // 创建会话
    let session_id = api.create_session();
    app.session_id = Some(session_id.clone());
    app.status_message = format!("会话已创建: {}", &session_id[..8]);

    // 获取可用的 agents
    let agents = api.list_agents().await;
    if !agents.is_empty() {
        app.available_agents = agents;
        app.current_agent = app.available_agents[0].clone();
    }

    let events = EventHandler::new(250);

    // 主循环
    while app.running {
        // 处理内部消息队列
        loop {
            let msg = if let Some(ref mut rx) = app.message_rx {
                rx.try_recv().ok()
            } else {
                None
            };
            
            match msg {
                Some(app_msg) => {
                    app.handle_app_message(app_msg);
                }
                None => break,
            }
        }

        // 渲染
        terminal.draw(|f| ui::render(f, &app))?;

        // 处理事件
        match events.next()? {
            TuiEvent::Quit => {
                app.running = false;
            }
            TuiEvent::Send => {
                if !app.input_buffer.is_empty() && !app.is_loading {
                    let message = app.input_buffer.clone();
                    
                    // 清空输入
                    app.input_buffer.clear();
                    
                    // 标记已开始对话
                    app.has_started_chat = true;
                    
                    // 添加用户消息
                    app.add_user_message(&message);
                    app.status_message = "正在思考...".to_string();
                    
                    // 发送消息并处理流式响应
                    let api_clone = api.clone();
                    let session_clone = session_id.clone();
                    let tx = app.message_tx.clone().unwrap();
                    
                    // 在后台任务中处理流式响应
                    tokio::spawn(async move {
                        // 设置加载状态
                        let _ = tx.send(AppMessage::SetLoading(true)).await;
                        let _ = tx.send(AppMessage::UpdateStatus("AI 正在回复...".to_string())).await;
                        
                        let request = ChatRequest {
                            session_id: session_clone,
                            message: message,
                            provider: None,
                            model: None,
                            agent: None,
                        };
                        
                        let mut full_response = String::new();
                        
                        match api_clone.chat_stream(request).await {
                            Ok(mut stream) => {
                                while let Some(chunk_result) = stream.next().await {
                                    match chunk_result {
                                        Ok(chunk) => {
                                            match chunk {
                                                AgentOutputChunk::Content { content } => {
                                                    full_response.push_str(&content);
                                                }
                                                AgentOutputChunk::ToolCall { name, .. } => {
                                                    let _ = tx.send(AppMessage::AddNotification(Notification {
                                                        notif_type: NotificationType::Info,
                                                        message: format!("调用工具: {}", name),
                                                        timestamp: Instant::now(),
                                                    })).await;
                                                }
                                                AgentOutputChunk::Finish { .. } => {
                                                    // 完成
                                                }
                                                _ => {}
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx.send(AppMessage::AddNotification(Notification {
                                                notif_type: NotificationType::Error,
                                                message: format!("错误: {:?}", e),
                                                timestamp: Instant::now(),
                                            })).await;
                                            break;
                                        }
                                    }
                                }
                                
                                // 添加完整的助手消息
                                if !full_response.is_empty() {
                                    let _ = tx.send(AppMessage::AddMessage(Message {
                                        msg_type: MessageType::Assistant,
                                        content: full_response,
                                        timestamp: Instant::now(),
                                    })).await;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::AddNotification(Notification {
                                    notif_type: NotificationType::Error,
                                    message: format!("聊天错误: {:?}", e),
                                    timestamp: Instant::now(),
                                })).await;
                            }
                        }
                        
                        // 取消加载状态
                        let _ = tx.send(AppMessage::SetLoading(false)).await;
                        let _ = tx.send(AppMessage::UpdateStatus("就绪".to_string())).await;
                    });
                }
            }
            TuiEvent::Key(key_event) => {
                match key_event.code {
                    crossterm::event::KeyCode::Char(c) => {
                        app.input_buffer.push(c);
                    }
                    crossterm::event::KeyCode::Backspace => {
                        app.input_buffer.pop();
                    }
                    crossterm::event::KeyCode::Tab => {
                        // Tab 切换 agent
                        app.next_agent();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // 清理终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
