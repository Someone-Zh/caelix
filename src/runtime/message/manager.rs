use crate::runtime::message::bus::MessageBus;
use crate::runtime::message::storage::StorageBackend;
use crate::runtime::message::types::{ActiveSpanInfo, Message, SessionState, SessionConfig, Status, MessageType};
use anyhow::Result;
use futures::Stream;
use futures::StreamExt;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

pub struct SessionManager {
    bus: MessageBus,
    storage: Arc<dyn StorageBackend>,
    // 内存状态：session_id -> state
    states: Arc<tokio::sync::RwLock<HashMap<String, SessionState>>>,
    // 存储消费者任务句柄
    _store_handle: JoinHandle<()>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("states", &self.states)
            .finish()
    }
}

impl SessionManager {
    pub fn new(bus: MessageBus, storage: Arc<dyn StorageBackend>) -> Self {
        let states = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let states_clone = states.clone();
        let storage_clone = storage.clone();
        let mut rx = bus.subscribe();

        // 启动存储消费者任务 (单线程顺序写入)
        let _store_handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        // 判断是否为通知消息
                        let is_notification = matches!(
                            msg.r#type,
                            MessageType::Info | MessageType::Error | 
                            MessageType::Warning | MessageType::Success |
                            MessageType::TaskStarted | MessageType::TaskCompleted |
                            MessageType::TaskFailed | MessageType::TaskProgress
                        );
                        
                        if is_notification {
                            // 存储到通知文件
                            if let Err(e) = storage_clone.append_notification(&msg).await {
                                eprintln!("[Storage Error] Failed to append notification: {}", e);
                            }
                        } else {
                            // 1. 更新内存状态
                            {
                                let mut states = states_clone.write().await;
                                let state: &mut SessionState = states.entry(msg.session_id.clone()).or_default();
                                
                                if msg.status == Status::Running {
                                    state.active_spans.insert(
                                        msg.span_id.clone(),
                                        ActiveSpanInfo {
                                            span_id: msg.span_id.clone(),
                                            parent_span_id: msg.parent_span_id.clone(),
                                            name: msg.name.clone(),
                                            status: msg.status.clone(),
                                            started_at: msg.timestamp,
                                        },
                                    );
                                } else if msg.status == Status::Done || msg.status == Status::Error {
                                    state.active_spans.remove(&msg.span_id);
                                }
                                
                                // 异步持久化 state (不阻塞主流程)
                                let state_clone = state.clone();
                                let storage_clone = storage_clone.clone();
                                let sess_id = msg.session_id.clone();
                                tokio::spawn(async move {
                                    let _ = storage_clone.save_state(&sess_id, &state_clone).await;
                                });
                            }

                            // 2. 持久化消息
                            if let Err(e) = storage_clone.append_message(&msg).await {
                                eprintln!("[Storage Error] Failed to append message: {}", e);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        Self {
            bus,
            storage,
            states,
            _store_handle,
        }
    }

    /// 获取消息总线引用 (用于生产者发送消息)
    pub fn bus(&self) -> &MessageBus {
        &self.bus
    }

    /// 订阅 Session: 返回 (历史消息列表, 实时消息流)
    pub async fn subscribe_session(
        &self,
        session_id: String,
    ) -> Result<(
        Vec<Message>,
        Pin<Box<dyn Stream<Item = Result<Message, broadcast::error::RecvError>> + Send>>,
    )> {
        // 1. 读取历史
        let history = self.storage.read_messages(&session_id).await?;

        // 2. 订阅实时
        let rx = self.bus.subscribe();
        let session_id_clone = session_id.clone();
        
        let stream = BroadcastStream::new(rx)
            .filter_map(move |res| {
                let keep = res.as_ref().map_or(true, |m| m.session_id == session_id_clone);
                std::future::ready(if keep { Some(res) } else { None })
            })
            .map(|item| {
                // 将 BroadcastStreamRecvError 转换为标准的 RecvError
                item.map_err(|e| match e {
                  BroadcastStreamRecvError::Lagged(n) => {
                        RecvError::Lagged(n)
                    }
                })
            });

        Ok((history, Box::pin(stream)))
    }

    /// 获取当前 Session 状态
    pub async fn get_session_state(&self, session_id: &str) -> SessionState {
        let states = self.states.read().await;
        states.get(session_id).cloned().unwrap_or_default()
    }

    // ========== 会话配置管理方法 ==========

    /// 创建新会话配置
    pub async fn create_session_config(&self, session_id: String) -> Result<String> {
        let config = SessionConfig::new(session_id.clone());
        
        {
            let mut states = self.states.write().await;
            let state = states.entry(session_id.clone()).or_default();
            state.config = Some(config);
        }
        
        Ok(session_id)
    }

    /// 获取会话配置
    pub async fn get_session_config(&self, session_id: &str) -> Option<SessionConfig> {
        let states = self.states.read().await;
        states.get(session_id).and_then(|state| state.config.clone())
    }

    /// 设置会话的提供者
    pub async fn set_session_provider(&self, session_id: &str, provider: &str) -> Result<()> {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(session_id) {
            if let Some(ref mut config) = state.config {
                config.provider = Some(provider.to_string());
                Ok(())
            } else {
                anyhow::bail!("Session {} has no config", session_id);
            }
        } else {
            anyhow::bail!("Session {} not found", session_id);
        }
    }

    /// 设置会话的模型
    pub async fn set_session_model(&self, session_id: &str, model: &str) -> Result<()> {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(session_id) {
            if let Some(ref mut config) = state.config {
                config.model = Some(model.to_string());
                Ok(())
            } else {
                anyhow::bail!("Session {} has no config", session_id);
            }
        } else {
            anyhow::bail!("Session {} not found", session_id);
        }
    }

    /// 设置会话的 agent
    pub async fn set_session_agent(&self, session_id: &str, agent: &str) -> Result<()> {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(session_id) {
            if let Some(ref mut config) = state.config {
                config.agent = Some(agent.to_string());
                Ok(())
            } else {
                anyhow::bail!("Session {} has no config", session_id);
            }
        } else {
            anyhow::bail!("Session {} not found", session_id);
        }
    }

    /// 检查会话是否存在
    pub async fn session_exists(&self, session_id: &str) -> bool {
        let states = self.states.read().await;
        states.contains_key(session_id)
    }

    /// 获取所有会话 ID
    pub async fn list_sessions(&self) -> Vec<String> {
        let states = self.states.read().await;
        states.keys().cloned().collect()
    }

    /// 获取会话的完整消息历史
    pub async fn get_session_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        self.storage.read_messages(session_id).await
    }

    /// 获取会话的通知消息
    pub async fn get_session_notifications(&self, session_id: &str) -> Result<Vec<Message>> {
        self.storage.read_notifications(session_id).await
    }

    /// 获取指定 stream_id 的所有消息(包括未完成的流)
    pub async fn get_messages_by_stream_id(&self, session_id: &str, stream_id: &str) -> Vec<Message> {
        // 从存储中读取所有消息
        let all_messages = self.storage.read_messages(session_id).await.unwrap_or_default();
        
        // 筛选出带有指定 stream_id 的消息
        all_messages.into_iter()
            .filter(|msg| {
                if let Some(meta) = &msg.meta {
                    meta.stream_id.as_deref() == Some(stream_id)
                } else {
                    false
                }
            })
            .collect()
    }

    /// 获取所有未完成的流式消息组
    pub async fn get_incomplete_streams(&self, session_id: &str) -> Vec<String> {
        // 从存储中读取所有消息
        let all_messages = self.storage.read_messages(session_id).await.unwrap_or_default();
        
        // 收集所有 stream_id
        let mut stream_ids: HashMap<String, bool> = HashMap::new();
        for msg in &all_messages {
            if let Some(meta) = &msg.meta {
                if let Some(stream_id) = &meta.stream_id {
                    // 如果找到 is_final=true 的消息，标记为完成
                    if meta.is_final {
                        stream_ids.insert(stream_id.clone(), true);
                    } else if !stream_ids.contains_key(stream_id) {
                        // 否则标记为未完成
                        stream_ids.insert(stream_id.clone(), false);
                    }
                }
            }
        }
        
        // 返回所有未完成的 stream_id
        stream_ids.into_iter()
            .filter_map(|(id, is_complete)| if !is_complete { Some(id) } else { None })
            .collect()
    }
}