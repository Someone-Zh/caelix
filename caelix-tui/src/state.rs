use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use super::commands::CommandHandler;
use caelix_api::task::TaskMeta;
use caelix_message::NotificationMessage;
use caelix_service::CaelixApiImpl;

/// 对话消息类型
#[derive(Debug, Clone, PartialEq)]
pub enum TuiMessageType {
    User,
    Assistant,
    #[allow(dead_code)] // 为将来使用预留
    System,
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct TuiMessage {
    pub msg_type: TuiMessageType,
    pub content: String,
    #[allow(dead_code)] // 为将来使用预留
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
    #[allow(dead_code)] // 为将来使用预留
    pub notif_type: NotificationType,
    #[allow(dead_code)] // 为将来使用预留
    pub message: String,
    #[allow(dead_code)] // 为将来使用预留
    pub timestamp: Instant,
}

/// 气泡通知（右下角短暂显示）
#[derive(Debug, Clone)]
pub struct BubbleNotification {
    pub message: String,
    pub notif_type: NotificationType,
    #[allow(dead_code)] // 为将来使用预留
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
    SessionList,  // Session列表弹窗
    ProviderList, // Provider列表弹窗
    ModelList,    // Model列表弹窗（二级）
}

/// 应用内部消息（用于异步任务与主循环通信）
#[derive(Debug, Clone)]
pub enum AppMessage {
    AddMessage(TuiMessage),
    AddNotification(Notification),
    SetLoading(bool),
    UpdateStatus(String),
    #[allow(dead_code)] // 为将来使用预留
    StreamContent(String), // 流式内容追加
    #[allow(dead_code)] // 为将来使用预留
    StartStreamingMessage, // 开始流式消息
    #[allow(dead_code)] // 为将来使用预留
    UpdateTasks(Vec<TaskMeta>), // 更新任务列表
    UpdateSessions(Vec<caelix_service::types::SessionSummary>), // 更新session列表
    UpdateProviders(Vec<caelix_service::types::ProviderInfo>),  // 更新provider列表
    UpdateProviderModels(Vec<String>),                          // 更新provider的models列表
}

/// TUI 应用状态
pub struct App {
    pub session_id: Option<String>,
    pub input_buffer: String,
    pub messages: Vec<TuiMessage>,        // 对话历史
    pub notifications: Vec<Notification>, // 通知队列（已废弃，保留兼容）
    pub scroll_offset: u16,
    pub current_provider: String,
    pub current_model: String,
    pub current_agent: String,
    pub available_agents: Vec<String>, // 可用的 agent 列表
    pub running: bool,
    pub is_loading: bool,                    // 是否正在加载 AI 响应
    pub loading_start_time: Option<Instant>, // 加载开始时间
    pub has_started_chat: bool,              // 是否已经开始对话（用于切换视图）
    pub status_message: String,              // 状态栏消息
    pub streaming_content: String,           // 当前流式内容缓冲区
    pub is_streaming: bool,                  // 是否正在流式接收
    // 用于异步任务通信的通道
    pub message_tx: Option<mpsc::Sender<AppMessage>>,
    pub message_rx: Option<mpsc::Receiver<AppMessage>>,
    // 新增字段
    pub tasks: Vec<TaskMeta>,                            // 当前任务列表
    pub notifications_history: Vec<NotificationMessage>, // 通知历史记录
    pub active_view: AppView,                            // 当前激活的视图
    pub message_bus_rx: Option<broadcast::Receiver<NotificationMessage>>, // 消息总线订阅者
    pub bubble_notifications: Vec<BubbleNotification>,   // 活跃的气泡通知
    #[allow(dead_code)] // 为将来使用预留
    pub active_streams: HashMap<String, String>, // stream_id -> 当前累积内容
    #[allow(dead_code)] // 为将来使用预留
    pub completed_streams: HashSet<String>, // 已完成的 stream_id
    // 视图栈管理（用于Esc返回）
    pub view_stack: Vec<AppView>, // 视图历史栈
    // Session/Provider/Model 选择相关
    pub sessions: Vec<caelix_service::types::SessionSummary>, // Session列表
    pub providers: Vec<caelix_service::types::ProviderInfo>,  // Provider列表
    pub selected_session_idx: usize,                          // 选中的session索引
    pub selected_provider_idx: usize,                         // 选中的provider索引
    pub selected_model_idx: usize,                            // 选中的model索引
    pub provider_models: Vec<String>,                         // 当前provider的models列表
    pub is_loading_sessions: bool,                            // 是否正在加载sessions
    pub is_loading_providers: bool,                           // 是否正在加载providers
    pub is_loading_models: bool,                              // 是否正在加载models
    // 命令自动补全相关
    pub available_commands: Vec<&'static str>, // 可用命令列表
    pub filtered_commands: Vec<String>,        // 过滤后的命令列表
    pub selected_command_idx: usize,           // 选中的命令索引
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
            message_bus_rx: None,
            bubble_notifications: Vec::new(),
            active_streams: HashMap::new(),
            completed_streams: HashSet::new(),
            // 视图栈和选择状态初始化
            view_stack: Vec::new(),
            sessions: Vec::new(),
            providers: Vec::new(),
            selected_session_idx: 0,
            selected_provider_idx: 0,
            selected_model_idx: 0,
            provider_models: Vec::new(),
            is_loading_sessions: false,
            is_loading_providers: false,
            is_loading_models: false,
            // 命令自动补全初始化
            available_commands: vec![
                "/quit",
                "/session",
                "/new",
                "/providers",
                "/models",
                "/tasks",
                "/notifications",
                "/chat",
                "/back",
            ],
            filtered_commands: Vec::new(),
            selected_command_idx: 0,
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
    #[allow(dead_code)] // 为将来使用预留
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
            AppMessage::UpdateSessions(sessions) => {
                // 更新session列表
                self.sessions = sessions;
                self.is_loading_sessions = false;
            }
            AppMessage::UpdateProviders(providers) => {
                // 更新provider列表
                self.providers = providers;
                self.is_loading_providers = false;
            }
            AppMessage::UpdateProviderModels(models) => {
                // 更新provider的models列表
                self.provider_models = models;
                self.is_loading_models = false;
            }
        }
    }

    /// 切换到下一个 agent
    pub fn next_agent(&mut self) {
        if self.available_agents.len() > 1 {
            let current_idx = self
                .available_agents
                .iter()
                .position(|a| a == &self.current_agent)
                .unwrap_or(0);
            let next_idx = (current_idx + 1) % self.available_agents.len();
            self.current_agent = self.available_agents[next_idx].clone();
        }
    }

    /// 显示气泡通知
    #[allow(dead_code)] // 为将来使用预留
    pub fn show_bubble_notification(&mut self, msg: &NotificationMessage) {
        use caelix_api::message::NotificationType as RuntimeNotificationType;

        // 根据消息类型决定气泡显示时长和是否持久化
        let (duration_secs, is_persistent) = match msg.r#type {
            RuntimeNotificationType::Error => (0, true), // 持久化
            RuntimeNotificationType::Warning => (5, false),
            RuntimeNotificationType::Info | RuntimeNotificationType::Success => (3, false),
        };

        let notif_type = match msg.r#type {
            RuntimeNotificationType::Info | RuntimeNotificationType::Success => {
                NotificationType::Info
            }
            RuntimeNotificationType::Error => NotificationType::Error,
            RuntimeNotificationType::Warning => NotificationType::Warning,
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
        self.bubble_notifications
            .retain(|n| n.is_persistent || now < n.expires_at);
    }

    /// 清除所有通知
    pub fn clear_all_notifications(&mut self) {
        self.notifications_history.clear();
    }

    /// 删除选中的通知
    pub fn delete_selected_notification(&mut self, index: usize) {
        if index < self.notifications_history.len() {
            self.notifications_history.remove(index);
        }
    }

    /// 推送到视图栈并切换视图
    pub fn push_view(&mut self, view: AppView) {
        // 将当前视图压入栈
        self.view_stack.push(self.active_view.clone());
        self.active_view = view;
    }

    /// 从视图栈弹出并返回上一层
    pub fn pop_view(&mut self) -> bool {
        if let Some(previous_view) = self.view_stack.pop() {
            self.active_view = previous_view;
            true
        } else {
            // 栈为空，已经在最底层
            false
        }
    }

    /// 更新过滤后的命令列表
    pub fn update_filtered_commands(&mut self) {
        CommandHandler::update_filtered_commands(self);
    }

    /// 选择当前高亮的命令并填充到输入框
    pub fn select_filtered_command(&mut self) -> bool {
        CommandHandler::select_filtered_command(self)
    }

    /// 处理命令
    pub fn handle_command(&mut self, api: Arc<CaelixApiImpl>, cmd: &str) {
        CommandHandler::handle_command(self, api, cmd);
    }

    /// 选择session并切换
    pub fn select_session(&mut self, api: Arc<CaelixApiImpl>) {
        CommandHandler::select_session(self, api);
    }

    /// 选择provider并切换
    pub fn select_provider(&mut self, api: Arc<CaelixApiImpl>) {
        CommandHandler::select_provider(self, api);
    }

    /// 在ModelList中选择provider（一级）
    pub fn select_provider_for_model(&mut self, api: Arc<CaelixApiImpl>) {
        CommandHandler::select_provider_for_model(self, api);
    }

    /// 选择model并切换
    pub fn select_model(&mut self, api: Arc<CaelixApiImpl>) {
        CommandHandler::select_model(self, api);
    }
}
