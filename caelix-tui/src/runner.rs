use std::sync::Arc;

use caelix_service::{CaelixApi, CaelixApiImpl};
use super::state::{App, AppView, AppMessage, NotificationType};
use super::events::{EventHandler, TuiEvent};
use super::views;

/// 运行 TUI 应用
pub async fn run_tui(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        Terminal,
    };

    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建应用状态
    let mut app = App::new();
    
    // 创建会话（需要异步调用）
    let session_id = api.create_session().await;
    app.session_id = Some(session_id.clone());
    app.status_message = format!("会话已创建: {}", &session_id[..8]);

    // 获取可用的 agents
    let agents = api.list_agents().await;
    if !agents.is_empty() {
        app.available_agents = agents;
        app.current_agent = app.available_agents[0].clone();
    }
    
    // 初始化消息总线订阅（订阅通知消息）
    let message_bus_rx = api.message_bus().subscribe_notification();
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
            // NotificationMessage 不包含流式数据，只显示为通知
            // 将通知添加到气泡通知中
            use caelix_api::message::NotificationType as RuntimeNotificationType;
            
            let notif_type = match msg.r#type {
                RuntimeNotificationType::Info => NotificationType::Info,
                RuntimeNotificationType::Success => NotificationType::Success,
                RuntimeNotificationType::Error => NotificationType::Error,
                RuntimeNotificationType::Warning => NotificationType::Warning,
            };
            
            // 创建气泡通知
            use std::time::Instant;
            use super::state::BubbleNotification;
            let bubble = BubbleNotification {
                message: msg.content.clone(),
                notif_type,
                created_at: Instant::now(),
                expires_at: Instant::now() + std::time::Duration::from_secs(5),
                is_persistent: false,
            };
            app.bubble_notifications.push(bubble);
            
            // 同时添加到通知历史
            app.notifications_history.push(msg);
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
        terminal.draw(|f| views::render(f, &app))?;

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
                    let tx = app.message_tx.clone().unwrap();
                    
                    // 在后台任务中处理异步调用（RuntimeContext 在 API 层内部管理）
                    tokio::spawn(async move {
                        // 设置加载状态
                        let _ = tx.send(AppMessage::SetLoading(true)).await;
                        let _ = tx.send(AppMessage::UpdateStatus("AI 正在回复...".to_string())).await;
                    });
                }
            }
            TuiEvent::NewLine => {
                // 普通Enter键：在输入框中插入换行符
                if app.active_view == AppView::Chat {
                    app.input_buffer.push('\n');
                }
            }
            TuiEvent::Key(key_event) => {
                handle_key_event(&mut app, api.clone(), key_event);
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

/// 处理按键事件
fn handle_key_event(app: &mut App, api: Arc<CaelixApiImpl>, key_event: crossterm::event::KeyEvent) {
    use crossterm::event::KeyCode;
    
    match key_event.code {
        KeyCode::Char('d') | KeyCode::Delete => {
            // 在通知历史视图中删除选中项（简化：删除最后一个）
            if app.active_view == AppView::Notifications && !app.notifications_history.is_empty() {
                let last_idx = app.notifications_history.len() - 1;
                app.delete_selected_notification(last_idx);
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            // 在通知历史视图中清除所有
            if app.active_view == AppView::Notifications {
                app.clear_all_notifications();
            }
        }
        KeyCode::Char(c) => {
            // 在聊天视图中，如果输入框为空且输入'/'，开始命令模式
            if app.active_view == AppView::Chat && app.input_buffer.is_empty() && c == '/' {
                app.input_buffer.push(c);
                app.update_filtered_commands();
            } else {
                // 正常输入字符
                app.input_buffer.push(c);
                // 如果当前以'/'开头，更新过滤列表
                if app.input_buffer.starts_with('/') {
                    app.update_filtered_commands();
                }
            }
        }
        KeyCode::Enter => {
            if app.active_view == AppView::Chat && app.input_buffer.starts_with('/') {
                // 在命令模式下
                if !app.filtered_commands.is_empty() {
                    // 如果有过滤的命令列表，先选择当前高亮的命令填充到输入框
                    app.select_filtered_command();
                    // 注意：这里不执行命令，只是填充，等待第二次回车
                } else {
                    // 没有匹配的命令，直接执行用户输入的命令
                    let cmd = app.input_buffer.clone();
                    app.handle_command(api.clone(), &cmd);
                    app.input_buffer.clear();
                }
            } else {
                // 根据当前视图处理Enter键
                match app.active_view {
                    AppView::SessionList => {
                        // 选择session
                        app.select_session(api.clone());
                    }
                    AppView::ProviderList => {
                        // 选择provider
                        app.select_provider(api.clone());
                    }
                    AppView::ModelList => {
                        // 在ModelList中，如果已经加载了models，则选择model
                        if !app.provider_models.is_empty() {
                            app.select_model(api.clone());
                        } else {
                            // 否则选择provider并加载其models
                            app.select_provider_for_model(api.clone());
                        }
                    }
                    _ => {
                        // 其他视图不处理
                    }
                }
            }
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
            // 如果当前以'/'开头，更新过滤列表
            if app.input_buffer.starts_with('/') {
                app.update_filtered_commands();
            }
        }
        KeyCode::Up => {
            // 上箭头：在列表中向上移动
            if app.active_view == AppView::Chat && app.input_buffer.starts_with('/') && !app.filtered_commands.is_empty() {
                // 在命令模式下，上下选择过滤后的命令
                if app.selected_command_idx > 0 {
                    app.selected_command_idx -= 1;
                } else {
                    // 循环到最后一个
                    app.selected_command_idx = app.filtered_commands.len() - 1;
                }
            } else {
                match app.active_view {
                    AppView::SessionList => {
                        if app.selected_session_idx > 0 {
                            app.selected_session_idx -= 1;
                        }
                    }
                    AppView::ProviderList => {
                        if app.selected_provider_idx > 0 {
                            app.selected_provider_idx -= 1;
                        }
                    }
                    AppView::ModelList => {
                        // 在 ModelList中，先判断是否已经加载了models
                        if !app.provider_models.is_empty() {
                            // 如果有models，在models中移动
                            if app.selected_model_idx > 0 {
                                app.selected_model_idx -= 1;
                            }
                        } else {
                            // 否则在providers中移动
                            if app.selected_provider_idx > 0 {
                                app.selected_provider_idx -= 1;
                                // 切换provider时加载其models
                                app.select_provider_for_model(api.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        KeyCode::Down => {
            // 下箭头：在列表中向下移动
            if app.active_view == AppView::Chat && app.input_buffer.starts_with('/') && !app.filtered_commands.is_empty() {
                // 在命令模式下，上下选择过滤后的命令
                if app.selected_command_idx + 1 < app.filtered_commands.len() {
                    app.selected_command_idx += 1;
                } else {
                    // 循环到第一个
                    app.selected_command_idx = 0;
                }
            } else {
                match app.active_view {
                    AppView::SessionList => {
                        if app.selected_session_idx + 1 < app.sessions.len() {
                            app.selected_session_idx += 1;
                        }
                    }
                    AppView::ProviderList => {
                        if app.selected_provider_idx + 1 < app.providers.len() {
                            app.selected_provider_idx += 1;
                        }
                    }
                    AppView::ModelList => {
                        if !app.provider_models.is_empty() {
                            // 在models中移动
                            if app.selected_model_idx + 1 < app.provider_models.len() {
                                app.selected_model_idx += 1;
                            }
                        } else {
                            // 在providers中移动
                            if app.selected_provider_idx + 1 < app.providers.len() {
                                app.selected_provider_idx += 1;
                                // 切换provider时加载其models
                                app.select_provider_for_model(api.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        KeyCode::Esc => {
            // ESC键：根据当前视图决定行为
            match app.active_view {
                AppView::Chat => {
                    // 在聊天视图，如果正在输入命令，清空输入框
                    if app.input_buffer.starts_with('/') {
                        app.input_buffer.clear();
                        app.filtered_commands.clear();
                    }
                    // 否则不处理（最后一层）
                }
                AppView::SessionList | AppView::ProviderList | AppView::ModelList => {
                    // 在弹窗视图中，返回上一层
                    app.pop_view();
                }
                _ => {
                    // 其他视图（Tasks, Notifications），返回聊天视图
                    app.view_stack.clear();
                    app.active_view = AppView::Chat;
                }
            }
        }
        KeyCode::Tab => {
            // Tab 切换 agent（仅在聊天视图）
            if app.active_view == AppView::Chat {
                app.next_agent();
            }
        }
        _ => {
            // 其他按键不处理
        }
    }
}
