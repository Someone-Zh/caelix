use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, List, ListItem},
};
use crate::backends::tui::app::{App, MessageType, NotificationType};

/// 渲染主界面
pub fn render(frame: &mut Frame, app: &App) {
    if !app.has_started_chat {
        // 初始视图：显示标题和输入框
        render_welcome_view(frame, app);
    } else {
        // 对话视图：显示历史、输入框和通知
        render_chat_view(frame, app);
    }
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
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // 输入框
    let input_block = Block::default()
        .title(" 输入消息 (Enter发送, Esc退出) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let input_text = format!("{}█", app.input_buffer);
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block)
        .style(Style::default().fg(Color::White));
    
    frame.render_widget(input_paragraph, chunks[2]);

    // 配置信息
    let config_text = format!(
        " Agent: {} | Provider: {} | Model: {} (Tab切换Agent) ",
        app.current_agent,
        app.current_provider,
        app.current_model
    );
    let config_bar = Paragraph::new(config_text)
        .style(Style::default().fg(Color::Yellow));
    
    frame.render_widget(config_bar, chunks[3]);

    // 状态栏
    let status_bar = Paragraph::new(format!(" {} ", app.status_message))
        .style(Style::default().fg(Color::Black).bg(Color::Gray));
    
    frame.render_widget(status_bar, chunks[4]);
}

/// 渲染对话视图
fn render_chat_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // 检查是否有通知需要显示
    let has_notifications = !app.notifications.is_empty();
    
    // 主布局：左侧对话区 + 右侧通知区（如果有通知）
    let main_chunks = if has_notifications {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(75),  // 左侧对话区
                Constraint::Percentage(25),  // 右侧通知区
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100),
            ])
            .split(area)
    };

    // 左侧：对话区域
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),     // 对话历史
            Constraint::Length(3),  // 输入框
            Constraint::Length(2),  // 配置信息
            Constraint::Length(1),  // 状态栏
        ])
        .split(main_chunks[0]);

    // 渲染对话历史
    render_messages(frame, app, left_chunks[0]);

    // 输入框
    let input_block = Block::default()
        .title(" 输入消息 (Enter发送, Tab切换Agent, Esc退出) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let input_text = format!("{}█", app.input_buffer);
    let mut input_style = Style::default().fg(Color::White);
    
    // 如果正在加载，改变输入框样式
    if app.is_loading {
        input_style = Style::default().fg(Color::DarkGray);
    }
    
    let input_paragraph = Paragraph::new(input_text)
        .block(input_block)
        .style(input_style);
    
    frame.render_widget(input_paragraph, left_chunks[1]);

    // 配置信息
    let config_text = format!(
        " Agent: {} | Provider: {} | Model: {} (Tab切换Agent) ",
        app.current_agent,
        app.current_provider,
        app.current_model
    );
    let config_bar = Paragraph::new(config_text)
        .style(Style::default().fg(Color::Yellow));
    
    frame.render_widget(config_bar, left_chunks[2]);

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
        .style(Style::default().fg(Color::Black).bg(Color::Gray));
    
    frame.render_widget(status_bar, left_chunks[3]);

    // 右侧：通知区域（如果有通知）
    if has_notifications {
        render_notifications(frame, app, main_chunks[1]);
    }
}

/// 渲染对话消息
fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let messages_block = Block::default()
        .title(" 对话历史 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // 构建消息列表
    let items: Vec<ListItem> = app.messages.iter().map(|msg| {
        let (prefix, style) = match msg.msg_type {
            MessageType::User => ("👤 你: ", Style::default().fg(Color::Green)),
            MessageType::Assistant => ("💬 AI: ", Style::default().fg(Color::White)),
            MessageType::System => ("⚙️ 系统: ", Style::default().fg(Color::Yellow)),
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
        .border_style(Style::default().fg(Color::Magenta));

    // 构建通知列表
    let items: Vec<ListItem> = app.notifications.iter().map(|notif| {
        let (icon, style) = match notif.notif_type {
            NotificationType::Info => ("ℹ️ ", Style::default().fg(Color::Blue)),
            NotificationType::Success => ("✅ ", Style::default().fg(Color::Green)),
            NotificationType::Error => ("❌ ", Style::default().fg(Color::Red)),
            NotificationType::Warning => ("⚠️ ", Style::default().fg(Color::Yellow)),
        };
        
        let content = format!("{}{}", icon, notif.message);
        ListItem::new(content).style(style)
    }).collect();

    let notification_list = List::new(items)
        .block(notifications_block);

    frame.render_widget(notification_list, area);
}
