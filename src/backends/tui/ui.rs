use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, List, ListItem, Wrap},
};
use crate::backends::tui::app::{App, TuiMessageType, NotificationType, AppView};
use crate::runtime::message::types::MessageType as RuntimeMessageType;
use crate::runtime::task::TaskStatus;

/// 渲染主界面
pub fn render(frame: &mut Frame, app: &App) {
    match app.active_view {
        AppView::Chat => {
            if !app.has_started_chat {
                render_welcome_view(frame, app);
            } else {
                render_chat_view(frame, app);
            }
        }
        AppView::Tasks => render_tasks_view(frame, app),
        AppView::Notifications => render_notifications_view(frame, app),
    }
    
    // 始终渲染气泡通知（在所有视图之上）
    render_bubble_notifications(frame, app);
}

/// 渲染欢迎视图（初始状态）
fn render_welcome_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // 垂直布局：标题、输入框、配置信息、状态栏
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题
            Constraint::Min(5),     // 空白
            Constraint::Length(3),  // 输入框
            Constraint::Length(2),  // 配置信息
            Constraint::Length(1),  // 状态栏
        ])
        .split(area);

    // 标题 - 居中显示 "Caelix"
    let title = Paragraph::new("Caelix")
        .style(Style::default().fg(Color::Rgb(86, 156, 214)).add_modifier(Modifier::BOLD))  // #569CD6 蓝色高亮核心关键词
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // 输入框
    let input_block = Block::default()
        .title(" 输入消息 (Enter发送, Esc退出) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(86, 156, 214)));  // #569CD6 蓝色边框
    
    let input_text = format!("{}█", app.input_buffer);
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block)
        .style(Style::default().fg(Color::Rgb(212, 212, 212)));  // #D4D4D4 普通文本
    
    frame.render_widget(input_paragraph, chunks[2]);

    // 配置信息（欢迎视图）
    let config_text = format!(
        " Agent: {} | Provider: {} | Model: {} (Tab切换Agent) ",
        app.current_agent,
        app.current_provider,
        app.current_model
    );
    let config_bar = Paragraph::new(config_text)
        .style(Style::default().fg(Color::Rgb(78, 201, 176)));  // #4EC9B0 青绿色用于配置信息
    
    frame.render_widget(config_bar, chunks[3]);

    // 状态栏
    let status_bar = Paragraph::new(format!(" {} ", app.status_message))
        .style(Style::default().fg(Color::Rgb(133, 133, 133)).bg(Color::Rgb(30, 30, 30)));  // #858585 辅助文本，#1E1E1E 背景
    
    frame.render_widget(status_bar, chunks[4]);
}

/// 渲染对话视图
fn render_chat_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // 全屏布局：对话历史 + 输入框 + 配置信息 + 状态栏
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),     // 对话历史
            Constraint::Length(3),  // 输入框
            Constraint::Length(2),  // 配置信息
            Constraint::Length(1),  // 状态栏
        ])
        .split(area);

    // 渲染对话历史
    render_messages(frame, app, chunks[0]);

    // 输入框
    let input_block = Block::default()
        .title(" 输入消息 (Enter发送, Tab切换Agent, /命令, Esc退出) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(86, 156, 214)));  // #569CD6 蓝色边框
    
    let input_text = if app.is_command_mode {
        format!("/{}█", app.command_buffer)
    } else {
        format!("{}█", app.input_buffer)
    };
    
    let mut input_style = Style::default().fg(Color::Rgb(212, 212, 212));  // #D4D4D4 普通文本
    
    // 如果正在加载，改变输入框样式
    if app.is_loading {
        input_style = Style::default().fg(Color::Rgb(133, 133, 133));  // #858585 辅助文本灰色
    }
    
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block)
        .style(input_style);
    
    frame.render_widget(input_paragraph, chunks[1]);

    // 配置信息（对话视图）
    let config_text = format!(
        " Agent: {} | Provider: {} | Model: {} (Tab切换Agent) ",
        app.current_agent,
        app.current_provider,
        app.current_model
    );
    let config_bar = Paragraph::new(config_text)
        .style(Style::default().fg(Color::Rgb(78, 201, 176)));  // #4EC9B0 青绿色用于配置信息
    
    frame.render_widget(config_bar, chunks[2]);

    // 状态栏（包含加载状态）
    let status_text = if app.is_loading {
        let elapsed = app.loading_start_time
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        let dots = ".".repeat((elapsed % 4) as usize);
        format!(" {} 加载中{} ", app.status_message, dots)
    } else {
        format!(" {} ", app.status_message)
    };
    
    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Rgb(133, 133, 133)).bg(Color::Rgb(30, 30, 30)));  // #858585 辅助文本，#1E1E1E 背景
    
    frame.render_widget(status_bar, chunks[3]);
}

/// 渲染对话消息
fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let messages_block = Block::default()
        .title(" 对话历史 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(86, 156, 214)));  // #569CD6 蓝色边框

    // 构建消息列表
    let items: Vec<ListItem> = app.messages.iter().enumerate().map(|(idx, msg)| {
        let (prefix, style) = match msg.msg_type {
            TuiMessageType::User => ("👤 你: ", Style::default().fg(Color::Rgb(78, 201, 176))),  // #4EC9B0 青绿色用户消息
            TuiMessageType::Assistant => {
                // 如果是最后一条消息且正在流式接收，添加闪烁光标提示
                let is_last_and_streaming = idx == app.messages.len() - 1 && app.is_streaming;
                let content = if is_last_and_streaming {
                    format!("{}▌", msg.content)  // 添加闪烁光标
                } else {
                    msg.content.clone()
                };
                return ListItem::new(format!("💬 AI: {}", content))
                    .style(Style::default().fg(Color::Rgb(212, 212, 212)));  // #D4D4D4 普通文本助手消息
            }
            TuiMessageType::System => ("⚙️ 系统: ", Style::default().fg(Color::Rgb(197, 134, 192))),  // #C586C0 紫色系统消息（交互提示）
        };
        
        let content = format!("{}{}", prefix, msg.content);
        ListItem::new(content).style(style)
    }).collect();

    let message_list = List::new(items)
        .block(messages_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_widget(message_list, area);
}

/// 渲染通知气泡
fn render_notifications(frame: &mut Frame, app: &App, area: Rect) {
    let notifications_block = Block::default()
        .title(" 通知 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(197, 134, 192)));  // #C586C0 紫色边框（交互提示）

    // 构建通知列表
    let items: Vec<ListItem> = app.notifications.iter().map(|notif| {
        let (icon, style) = match notif.notif_type {
            NotificationType::Info => ("ℹ️ ", Style::default().fg(Color::Rgb(86, 156, 214))),  // #569CD6 蓝色信息
            NotificationType::Success => ("✅ ", Style::default().fg(Color::Rgb(78, 201, 176))),  // #4EC9B0 青绿色成功
            NotificationType::Error => ("❌ ", Style::default().fg(Color::Rgb(212, 212, 212))),  // #D4D4D4 普通文本错误（需要醒目）
            NotificationType::Warning => ("⚠️ ", Style::default().fg(Color::Rgb(197, 134, 192))),  // #C586C0 紫色警告（交互提示）
        };
        
        let content = format!("{}{}", icon, notif.message);
        ListItem::new(content).style(style)
    }).collect();

    let notification_list = List::new(items)
        .block(notifications_block);

    frame.render_widget(notification_list, area);
}

/// 渲染任务列表视图
fn render_tasks_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // 垂直布局：标题区 + 任务列表 + 底部提示
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题
            Constraint::Min(5),     // 任务列表
            Constraint::Length(1),  // 提示
        ])
        .split(area);
    
    // 标题
    let title = Paragraph::new("任务列表")
        .style(Style::default().fg(Color::Rgb(86, 156, 214)).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);
    
    // 任务列表
    render_task_list(frame, app, chunks[1]);
    
    // 底部提示
    let hint = Paragraph::new(" 按 /chat 返回聊天视图 ")
        .style(Style::default().fg(Color::Rgb(133, 133, 133)));
    frame.render_widget(hint, chunks[2]);
}

fn render_task_list(frame: &mut Frame, app: &App, area: Rect) {
    let tasks_block = Block::default()
        .title(format!(" 活动任务 ({}) ", app.tasks.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(86, 156, 214)));
    
    if app.tasks.is_empty() {
        let empty_msg = Paragraph::new("暂无活动任务")
            .block(tasks_block)
            .style(Style::default().fg(Color::Rgb(133, 133, 133)))
            .alignment(Alignment::Center);
        frame.render_widget(empty_msg, area);
        return;
    }
    
    let items: Vec<ListItem> = app.tasks.iter().map(|task| {
        let status_icon = match task.status {
            TaskStatus::Running => "⏳",
            TaskStatus::Completed => "✅",
            TaskStatus::Failed(_) => "❌",
            _ => "⏸️",
        };
        
        let status_str = match task.status {
            TaskStatus::Pending => "pending",
            TaskStatus::Scheduled => "scheduled",
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed(_) => "failed",
            TaskStatus::Cancelled => "cancelled",
        };
        
        let progress_bar = if let Some(progress) = task.progress {
            if progress > 0.0 {
                format!("[{:.0}%]", progress * 100.0)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        
        let content = format!(
            "{} {} {} {}",
            status_icon,
            task.task_type_name,
            progress_bar,
            status_str
        );
        
        ListItem::new(content).style(Style::default().fg(Color::Rgb(212, 212, 212)))
    }).collect();
    
    let task_list = List::new(items).block(tasks_block);
    frame.render_widget(task_list, area);
}

/// 渲染通知历史视图
fn render_notifications_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题
            Constraint::Min(5),     // 通知列表
            Constraint::Length(1),  // 提示
        ])
        .split(area);
    
    // 标题
    let title = Paragraph::new("通知历史")
        .style(Style::default().fg(Color::Rgb(197, 134, 192)).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);
    
    // 通知列表
    render_notification_history_list(frame, app, chunks[1]);
    
    // 底部提示
    let hint = Paragraph::new(" D:删除选中 | C:清除全部 | /chat:返回 ")
        .style(Style::default().fg(Color::Rgb(133, 133, 133)));
    frame.render_widget(hint, chunks[2]);
}

fn render_notification_history_list(frame: &mut Frame, app: &App, area: Rect) {
    let notif_block = Block::default()
        .title(format!(" 通知记录 ({}) ", app.notifications_history.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(197, 134, 192)));
    
    if app.notifications_history.is_empty() {
        let empty_msg = Paragraph::new("暂无通知记录")
            .block(notif_block)
            .style(Style::default().fg(Color::Rgb(133, 133, 133)))
            .alignment(Alignment::Center);
        frame.render_widget(empty_msg, area);
        return;
    }
    
    let items: Vec<ListItem> = app.notifications_history.iter().enumerate().map(|(idx, msg)| {
        let (icon, style) = match msg.r#type {
            RuntimeMessageType::Info => ("ℹ️ ", Style::default().fg(Color::Rgb(86, 156, 214))),
            RuntimeMessageType::Success => ("✅ ", Style::default().fg(Color::Rgb(78, 201, 176))),
            RuntimeMessageType::Error => ("❌ ", Style::default().fg(Color::Rgb(212, 212, 212))),
            RuntimeMessageType::Warning => ("⚠️ ", Style::default().fg(Color::Rgb(197, 134, 192))),
            RuntimeMessageType::TaskStarted => ("🚀 ", Style::default().fg(Color::Rgb(78, 201, 176))),
            RuntimeMessageType::TaskCompleted => ("✓ ", Style::default().fg(Color::Rgb(78, 201, 176))),
            RuntimeMessageType::TaskFailed => ("✗ ", Style::default().fg(Color::Rgb(212, 212, 212))),
            RuntimeMessageType::TaskProgress => ("⏳ ", Style::default().fg(Color::Rgb(86, 156, 214))),
            _ => ("• ", Style::default().fg(Color::Rgb(212, 212, 212))),
        };
        
        let time_str = msg.timestamp.format("%H:%M:%S").to_string();
        let content = format!("{} [{}] {}", icon, time_str, msg.content);
        ListItem::new(content).style(style)
    }).collect();
    
    let notif_list = List::new(items).block(notif_block);
    frame.render_widget(notif_list, area);
}

/// 渲染气泡通知（右下角）
fn render_bubble_notifications(frame: &mut Frame, app: &App) {
    if app.bubble_notifications.is_empty() {
        return;
    }
    
    let area = frame.area();
    
    // 计算气泡位置（右下角）
    let max_width = 50u16.min(area.width.saturating_sub(4));
    let bubble_height = (app.bubble_notifications.len() as u16 * 3).min(area.height.saturating_sub(4));
    
    let bubble_area = Rect {
        x: area.width.saturating_sub(max_width + 2),
        y: area.height.saturating_sub(bubble_height + 2),
        width: max_width,
        height: bubble_height,
    };
    
    // 渲染每个气泡
    for (i, bubble) in app.bubble_notifications.iter().enumerate() {
        if i as u16 * 3 >= bubble_area.height {
            break; // 超出显示区域
        }
        
        let bubble_rect = Rect {
            x: bubble_area.x,
            y: bubble_area.y + (i as u16 * 3),
            width: bubble_area.width,
            height: 3,
        };
        
        let (bg_color, border_color) = match bubble.notif_type {
            NotificationType::Info => (Color::Rgb(30, 60, 90), Color::Rgb(86, 156, 214)),
            NotificationType::Success => (Color::Rgb(30, 90, 60), Color::Rgb(78, 201, 176)),
            NotificationType::Error => (Color::Rgb(90, 30, 30), Color::Rgb(212, 212, 212)),
            NotificationType::Warning => (Color::Rgb(90, 60, 30), Color::Rgb(197, 134, 192)),
        };
        
        let bubble_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(bg_color));
        
        let text = Paragraph::new(bubble.message.clone())
            .block(bubble_block)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true });
        
        frame.render_widget(text, bubble_rect);
    }
}
