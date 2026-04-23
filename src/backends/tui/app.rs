use std::sync::Arc;
use std::time::Instant;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::broadcast;
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
use crate::runtime::message::types::{Message as RuntimeMessage, MessageType as RuntimeMessageType};
use crate::runtime::TaskMeta;
use super::events::{EventHandler, TuiEvent};
use super::ui;

/// 对话消息类型
#[derive(Debug, Clone, PartialEq)]
pub enum TuiMessageType {
    User,
    Assistant,
    System,
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct TuiMessage {
    pub msg_type: TuiMessageType,
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

/// 气泡通知（右下角短暂显示）
#[derive(Debug, Clone)]
pub struct BubbleNotification {
    pub message: String,
    pub notif_type: NotificationType,
    pub created_at: Instant,
    pub expires_at: Instant,
    pub is_persistent: bool,
}

/// 应用视图枚举
#[derive(Debug, Clone, PartialEq)]
pub enum AppView {
    Chat,
    Tasks,
    Notifications,
}

/// TUI 应用状态
pub struct App {
    pub session_id: Option<String>,
    pub input_buffer: String,
    pub messages: Vec<TuiMessage>,  // 对话历史
    pub notifications: Vec<Notification>,  // 通知队列（已废弃，保留兼容）
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
    pub streaming_content: String,  // 当前流式内容缓冲区
    pub is_streaming: bool,  // 是否正在流式接收
    // 用于异步任务通信的通道
    pub message_tx: Option<mpsc::Sender<AppMessage>>,
    pub message_rx: Option<mpsc::Receiver<AppMessage>>,
    // 新增字段
    pub tasks: Vec<TaskMeta>,              // 当前任务列表
    pub notifications_history: Vec<RuntimeMessage>, // 通知历史记录
    pub active_view: AppView,               // 当前激活的视图
    pub command_buffer: String,             // 命令输入缓冲区
    pub is_command_mode: bool,              // 是否处于命令模式
    pub message_bus_rx: Option<broadcast::Receiver<RuntimeMessage>>, // 消息总线订阅者
    pub bubble_notifications: Vec<BubbleNotification>, // 活跃的气泡通知
}

/// 应用内部消息（用于异步任务与主循环通信）
#[derive(Debug, Clone)]
pub enum AppMessage {
    AddMessage(TuiMessage),
    AddNotification(Notification),
    SetLoading(bool),
    UpdateStatus(String),
    StreamContent(String),  // 流式内容追加
    StartStreamingMessage,  // 开始流式消息
    UpdateTasks(Vec<TaskMeta>),  // 更新任务列表
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
            streaming_content: String::new(),
            is_streaming: false,
            message_tx: Some(tx),
            message_rx: Some(rx),
            // 新增字段初始化
            tasks: Vec::new(),
            notifications_history: Vec::new(),
            active_view: AppView::Chat,
            command_buffer: String::new(),
            is_command_mode: false,
            message_bus_rx: None,
            bubble_notifications: Vec::new(),
        }
    }

    /// 添加对话消息
    pub fn add_message(&mut self, msg: TuiMessage) {
        self.messages.push(msg);
        self.scroll_offset = self.messages.len() as u16;
    }

    /// 添加用户消息
    pub fn add_user_message(&mut self, content: &str) {
        self.add_message(TuiMessage {
            msg_type: TuiMessageType::User,
            content: content.to_string(),
            timestamp: Instant::now(),
        });
    }

    /// 添加助手消息
    pub fn add_assistant_message(&mut self, content: &str) {
        self.add_message(TuiMessage {
            msg_type: TuiMessageType::Assistant,
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
                    // 加载结束时，也结束流式状态
                    self.is_streaming = false;
                }
            }
            AppMessage::UpdateStatus(status) => {
                self.status_message = status;
            }
            AppMessage::StartStreamingMessage => {
                // 开始新的流式消息，清空缓冲区
                self.streaming_content.clear();
                self.is_streaming = true;
                // 添加一个空的助手消息作为占位符
                self.add_message(TuiMessage {
                    msg_type: TuiMessageType::Assistant,
                    content: String::new(),
                    timestamp: Instant::now(),
                });
            }
            AppMessage::StreamContent(content) => {
                // 追加流式内容
                self.streaming_content.push_str(&content);
                // 更新最后一条消息的内容（如果存在）
                if let Some(last_msg) = self.messages.last_mut() {
                    if last_msg.msg_type == TuiMessageType::Assistant {
                        last_msg.content = self.streaming_content.clone();
                        // 自动滚动到最新消息
                        self.scroll_offset = self.messages.len() as u16;
                    }
                }
            }
            AppMessage::UpdateTasks(tasks) => {
                // 更新任务列表
                self.tasks = tasks;
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
    
    /// 显示气泡通知
    pub fn show_bubble_notification(&mut self, msg: &RuntimeMessage) {
        // 根据消息类型决定气泡显示时长和是否持久化
        let (duration_secs, is_persistent) = match msg.r#type {
            RuntimeMessageType::Error | RuntimeMessageType::TaskFailed => (0, true), // 持久化
            RuntimeMessageType::Warning => (5, false),
            _ => (3, false), // Info, Success, TaskStarted, etc.
        };
        
        let notif_type = match msg.r#type {
            RuntimeMessageType::Info | RuntimeMessageType::TaskStarted | RuntimeMessageType::TaskProgress => NotificationType::Info,
            RuntimeMessageType::Success | RuntimeMessageType::TaskCompleted => NotificationType::Success,
            RuntimeMessageType::Error | RuntimeMessageType::TaskFailed => NotificationType::Error,
            RuntimeMessageType::Warning => NotificationType::Warning,
            _ => NotificationType::Info,
        };
        
        let now = Instant::now();
        self.bubble_notifications.push(BubbleNotification {
            message: msg.content.clone(),
            notif_type,
            created_at: now,
            expires_at: now + std::time::Duration::from_secs(duration_secs),
            is_persistent,
        });
    }
    
    /// 清理过期的气泡通知
    pub fn cleanup_expired_bubbles(&mut self) {
        let now = Instant::now();
        self.bubble_notifications.retain(|n| {
            n.is_persistent || now < n.expires_at
        });
    }
    
    /// 处理命令
    pub fn handle_command(&mut self, cmd: &str) {
        match cmd {
            "/tasks" => {
                self.active_view = AppView::Tasks;
                self.is_command_mode = false;
                self.command_buffer.clear();
            }
            "/notifications" => {
                self.active_view = AppView::Notifications;
                self.is_command_mode = false;
                self.command_buffer.clear();
            }
            "/chat" | "/back" => {
                self.active_view = AppView::Chat;
                self.is_command_mode = false;
                self.command_buffer.clear();
            }
            _ => {
                // 未知命令，显示错误提示
                self.add_notification(Notification {
                    notif_type: NotificationType::Error,
                    message: format!("未知命令: {}", cmd),
                    timestamp: Instant::now(),
                });
                self.is_command_mode = false;
                self.command_buffer.clear();
            }
        }
    }
    
    /// 删除选中的通知
    pub fn delete_selected_notification(&mut self, index: usize) {
        if index < self.notifications_history.len() {
            self.notifications_history.remove(index);
        }
    }
    
    /// 清除所有通知
    pub fn clear_all_notifications(&mut self) {
        self.notifications_history.clear();
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
    
    // 初始化消息总线订阅
    let message_bus_rx = api.message_bus().subscribe();
    app.message_bus_rx = Some(message_bus_rx);
    
    // 加载初始任务列表
    if let Ok(tasks) = api.list_tasks(Some(&session_id)).await {
        app.tasks = tasks;
    }
    
    // 加载通知历史
    if let Ok(notifs) = api.get_session_notifications(&session_id).await {
        app.notifications_history = notifs;
    }

    let events = EventHandler::new(250);

    // 主循环
    while app.running {
        // 处理消息总线（任务通知等）
        let mut bus_messages = Vec::new();
        if let Some(ref mut rx) = app.message_bus_rx {
            loop {
                match rx.try_recv() {
                    Ok(msg) => {
                        bus_messages.push(msg);
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(_) => break,
                }
            }
        }
        
        // 处理收集到的消息
        for msg in bus_messages {
            // 判断是否为任务相关通知
            if matches!(msg.r#type, 
                RuntimeMessageType::TaskStarted | RuntimeMessageType::TaskCompleted | 
                RuntimeMessageType::TaskFailed | RuntimeMessageType::TaskProgress)
            {
                // 更新任务列表（简化：收到任何任务消息就重新获取列表）
                if let Some(session_id) = &app.session_id {
                    let api_clone = api.clone();
                    let session_clone = session_id.clone();
                    let tx = app.message_tx.clone();
                    tokio::spawn(async move {
                        if let Ok(tasks) = api_clone.list_tasks(Some(&session_clone)).await {
                            if let Some(tx) = tx {
                                let _ = tx.send(AppMessage::UpdateTasks(tasks)).await;
                            }
                        }
                    });
                }
                // 添加到通知历史
                app.notifications_history.push(msg.clone());
                // 显示右下角气泡通知
                app.show_bubble_notification(&msg);
            } else if matches!(msg.r#type,
                RuntimeMessageType::Info | RuntimeMessageType::Error |
                RuntimeMessageType::Warning | RuntimeMessageType::Success)
            {
                // 通用通知也加入历史并显示气泡
                app.notifications_history.push(msg.clone());
                app.show_bubble_notification(&msg);
            }
        }
        
        // 清理过期的气泡通知
        app.cleanup_expired_bubbles();
        
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
                        
                        match api_clone.chat_stream(request).await {
                            Ok(mut stream) => {
                                // 开始流式消息
                                let _ = tx.send(AppMessage::StartStreamingMessage).await;
                                
                                while let Some(chunk_result) = stream.next().await {
                                    match chunk_result {
                                        Ok(chunk) => {
                                            match chunk {
                                                AgentOutputChunk::Content { content } => {
                                                    // 立即发送内容更新，实现流式显示
                                                    let _ = tx.send(AppMessage::StreamContent(content)).await;
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
                    crossterm::event::KeyCode::Char('/') => {
                        // 进入命令模式
                        if !app.is_command_mode && app.active_view == AppView::Chat {
                            app.is_command_mode = true;
                            app.command_buffer.clear();
                        }
                    }
                    crossterm::event::KeyCode::Enter => {
                        if app.is_command_mode {
                            // 执行命令（先克隆避免借用冲突）
                            let cmd = app.command_buffer.clone();
                            app.handle_command(&cmd);
                        } else if !app.input_buffer.is_empty() && !app.is_loading {
                            // 发送消息逻辑（原有代码在TuiEvent::Send中处理）
                            // 这里不处理，让Send事件处理
                        }
                    }
                    crossterm::event::KeyCode::Char('d') | crossterm::event::KeyCode::Delete => {
                        // 在通知历史视图中删除选中项（简化：删除最后一个）
                        if app.active_view == AppView::Notifications && !app.notifications_history.is_empty() {
                            let last_idx = app.notifications_history.len() - 1;
                            app.delete_selected_notification(last_idx);
                        }
                    }
                    crossterm::event::KeyCode::Char('c') | crossterm::event::KeyCode::Char('C') => {
                        // 在通知历史视图中清除所有
                        if app.active_view == AppView::Notifications {
                            app.clear_all_notifications();
                        }
                    }
                    crossterm::event::KeyCode::Esc => {
                        // ESC退出命令模式或应用
                        if app.is_command_mode {
                            app.is_command_mode = false;
                            app.command_buffer.clear();
                        } else {
                            app.running = false;
                        }
                    }
                    crossterm::event::KeyCode::Char(c) => {
                        if app.is_command_mode {
                            app.command_buffer.push(c);
                        } else {
                            app.input_buffer.push(c);
                        }
                    }
                    crossterm::event::KeyCode::Backspace => {
                        if app.is_command_mode {
                            app.command_buffer.pop();
                        } else {
                            app.input_buffer.pop();
                        }
                    }
                    crossterm::event::KeyCode::Tab => {
                        // Tab 切换 agent（仅在聊天视图）
                        if app.active_view == AppView::Chat {
                            app.next_agent();
                        }
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
