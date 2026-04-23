use std::sync::Arc;
use std::time::Instant;

use crate::api::{CaelixApi, CaelixApiImpl};
use super::state::{App, AppView, Notification, NotificationType, TuiMessage, TuiMessageType};

/// 命令处理器
pub struct CommandHandler;

impl CommandHandler {
    /// 更新过滤后的命令列表
    pub fn update_filtered_commands(app: &mut App) {
        // 移除开头的 '/' 进行匹配
        let prefix = if app.input_buffer.starts_with('/') {
            &app.input_buffer[1..]
        } else {
            &app.input_buffer
        };
        
        let prefix_lower = prefix.to_lowercase();
        app.filtered_commands = app.available_commands
            .iter()
            .filter(|cmd| cmd.to_lowercase().starts_with(&prefix_lower))
            .map(|cmd| cmd.to_string())
            .collect();
        app.selected_command_idx = 0;
    }
    
    /// 选择当前高亮的命令并填充到输入框
    pub fn select_filtered_command(app: &mut App) -> bool {
        if !app.filtered_commands.is_empty() && app.selected_command_idx < app.filtered_commands.len() {
            app.input_buffer = app.filtered_commands[app.selected_command_idx].clone();
            // 更新过滤列表（此时应该只有一个匹配项）
            Self::update_filtered_commands(app);
            true
        } else {
            false
        }
    }
    
    /// 处理命令
    pub fn handle_command(app: &mut App, api: Arc<CaelixApiImpl>, cmd: &str) {
        // 提取命令（去除开头的 '/'）
        let command = if cmd.starts_with('/') {
            &cmd[1..]
        } else {
            cmd
        };
        
        match command {
            "quit" => {
                app.running = false;
            }
            "tasks" => {
                app.push_view(AppView::Tasks);
            }
            "notifications" => {
                app.push_view(AppView::Notifications);
            }
            "chat" | "back" => {
                // 返回聊天视图，清空视图栈
                app.view_stack.clear();
                app.active_view = AppView::Chat;
            }
            "session" => {
                // 加载session列表
                app.push_view(AppView::SessionList);
                app.is_loading_sessions = true;
                app.selected_session_idx = 0;
                
                let api_clone = api.clone();
                let tx = app.message_tx.clone();
                tokio::spawn(async move {
                    if let Ok(sessions) = api_clone.list_sessions().await {
                        if let Some(tx) = tx {
                            let _ = tx.send(super::state::AppMessage::UpdateSessions(sessions)).await;
                        }
                    }
                });
            }
            "new" => {
                // 创建新session
                let new_session_id = api.create_session();
                app.session_id = Some(new_session_id.clone());
                app.status_message = format!("新会话已创建: {}", &new_session_id[..8]);
                
                // 清空当前对话
                app.messages.clear();
                app.has_started_chat = false;
                
                // 返回聊天视图
                app.view_stack.clear();
                app.active_view = AppView::Chat;
            }
            "providers" => {
                // 加载provider列表
                app.push_view(AppView::ProviderList);
                app.is_loading_providers = true;
                app.selected_provider_idx = 0;
                
                let api_clone = api.clone();
                let tx = app.message_tx.clone();
                tokio::spawn(async move {
                    if let Ok(providers) = api_clone.get_providers().await {
                        if let Some(tx) = tx {
                            let _ = tx.send(super::state::AppMessage::UpdateProviders(providers)).await;
                        }
                    }
                });
            }
            "models" => {
                // 显示provider和model的二级列表
                app.push_view(AppView::ModelList);
                app.is_loading_providers = true;
                app.selected_provider_idx = 0;
                app.selected_model_idx = 0;
                
                let api_clone = api.clone();
                let tx = app.message_tx.clone();
                tokio::spawn(async move {
                    if let Ok(providers) = api_clone.get_providers().await {
                        if let Some(tx) = tx {
                            let _ = tx.send(super::state::AppMessage::UpdateProviders(providers)).await;
                        }
                    }
                });
            }
            _ => {
                // 未知命令，显示错误提示
                app.add_notification(Notification {
                    notif_type: NotificationType::Error,
                    message: format!("未知命令: /{}", command),
                    timestamp: Instant::now(),
                });
            }
        }
    }
    
    /// 选择session并切换
    pub fn select_session(app: &mut App, api: Arc<CaelixApiImpl>) {
        if app.selected_session_idx < app.sessions.len() {
            let session = &app.sessions[app.selected_session_idx];
            app.session_id = Some(session.session_id.clone());
            app.status_message = format!("已切换到会话: {}", &session.session_id[..8]);
            
            // 清空当前对话并加载新会话的消息
            app.messages.clear();
            app.has_started_chat = false;
            
            // 异步加载会话消息
            let api_clone = api.clone();
            let session_id = session.session_id.clone();
            let tx = app.message_tx.clone();
            tokio::spawn(async move {
                if let Ok(messages) = api_clone.get_session_messages(&session_id).await {
                    if let Some(tx) = tx {
                        for msg in messages {
                            let tui_msg = TuiMessage {
                                msg_type: match msg.role {
                                    crate::runtime::message::types::Role::User => TuiMessageType::User,
                                    crate::runtime::message::types::Role::Agent => TuiMessageType::Assistant,
                                    _ => TuiMessageType::System,
                                },
                                content: msg.content,
                                timestamp: Instant::now(),
                            };
                            let _ = tx.send(super::state::AppMessage::AddMessage(tui_msg)).await;
                        }
                    }
                }
            });
            
            // 返回聊天视图
            app.view_stack.clear();
            app.active_view = AppView::Chat;
        }
    }
    
    /// 选择provider并切换
    pub fn select_provider(app: &mut App, api: Arc<CaelixApiImpl>) {
        if app.selected_provider_idx < app.providers.len() {
            let provider = &app.providers[app.selected_provider_idx];
            app.current_provider = provider.name.clone();
            
            // 如果有session，设置session的provider
            if let Some(session_id) = &app.session_id {
                let api_clone = api.clone();
                let session_clone = session_id.clone();
                let provider_clone = provider.name.clone();
                tokio::spawn(async move {
                    let _ = api_clone.set_session_provider(&session_clone, &provider_clone).await;
                });
            }
            
            app.status_message = format!("已切换提供者: {}", provider.name);
            
            // 返回聊天视图
            app.view_stack.clear();
            app.active_view = AppView::Chat;
        }
    }
    
    /// 在ModelList中选择provider（一级）
    pub fn select_provider_for_model(app: &mut App, api: Arc<CaelixApiImpl>) {
        if app.selected_provider_idx < app.providers.len() {
            let provider = &app.providers[app.selected_provider_idx];
            
            // 加载该provider的models
            app.is_loading_models = true;
            app.selected_model_idx = 0;
            
            let api_clone = api.clone();
            let provider_name = provider.name.clone();
            let tx = app.message_tx.clone();
            tokio::spawn(async move {
                if let Ok(models) = api_clone.get_provider_models(&provider_name).await {
                    if let Some(tx) = tx {
                        let _ = tx.send(super::state::AppMessage::UpdateProviderModels(models)).await;
                    }
                }
            });
        }
    }
    
    /// 选择model并切换
    pub fn select_model(app: &mut App, api: Arc<CaelixApiImpl>) {
        if app.selected_model_idx < app.provider_models.len() {
            let model = &app.provider_models[app.selected_model_idx];
            app.current_model = model.clone();
            
            // 如果有session，设置session的model
            if let Some(session_id) = &app.session_id {
                let api_clone = api.clone();
                let session_clone = session_id.clone();
                let model_clone = model.clone();
                tokio::spawn(async move {
                    let _ = api_clone.set_session_model(&session_clone, &model_clone).await;
                });
            }
            
            app.status_message = format!("已切换模型: {}", model);
            
            // 返回聊天视图
            app.view_stack.clear();
            app.active_view = AppView::Chat;
        }
    }
}
