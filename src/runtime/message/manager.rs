//! Session Manager 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use crate::runtime::message::bus::MessageBus;
use crate::runtime::message::storage::StorageBackend;
use crate::runtime::message::agent_message::{AgentMessage, AgentMessageType};
use crate::runtime::message::notification_message::NotificationMessage;
use crate::runtime::message::task_message::TaskMessage;
use crate::runtime::message::types::{SessionState, SessionConfig};
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
    // Agent 消息缓冲：(session_id, span_id) -> Vec<AgentMessage>
    agent_buffers: Arc<tokio::sync::RwLock<HashMap<(String, String), Vec<AgentMessage>>>>,
    // Notification 消息缓冲：session_id -> Vec<NotificationMessage>
    notification_buffers: Arc<tokio::sync::RwLock<HashMap<String, Vec<NotificationMessage>>>>,
    // Task 消息缓冲：session_id -> Vec<TaskMessage>
    task_buffers: Arc<tokio::sync::RwLock<HashMap<String, Vec<TaskMessage>>>>,
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
        let agent_buffers = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let notification_buffers = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let task_buffers = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        
        let agent_buffers_clone = agent_buffers.clone();
        let notification_buffers_clone = notification_buffers.clone();
        let task_buffers_clone = task_buffers.clone();
        let storage_clone = storage.clone();
        
        let mut agent_rx = bus.subscribe_agent();
        let mut notification_rx = bus.subscribe_notification();
        let mut task_rx = bus.subscribe_task();

        // 启动存储消费者任务 (单线程顺序写入)
        let _store_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 处理 Agent 消息
                    Ok(msg) = agent_rx.recv() => {
                        match msg.r#type {
                            AgentMessageType::Msg => {
                                // 持久化 Msg 类型
                                if let Err(e) = storage_clone.append_agent_message(&msg).await {
                                    eprintln!("[Storage Error] Failed to append agent message: {}", e);
                                }
                            }
                            AgentMessageType::Chunk => {
                                // 积累 Chunk 消息
                                let key = (msg.session_id.clone(), msg.span_id.clone());
                                let mut buffers = agent_buffers_clone.write().await;
                                buffers.entry(key).or_insert_with(Vec::new).push(msg);
                            }
                            AgentMessageType::ChunkEnd => {
                                // 清空该 span_id 的缓冲
                                let key = (msg.session_id.clone(), msg.span_id.clone());
                                let mut buffers = agent_buffers_clone.write().await;
                                buffers.remove(&key);
                            }
                        }
                    }
                    // 处理 Notification 消息
                    Ok(msg) = notification_rx.recv() => {
                        // 积累通知消息
                        let mut buffers = notification_buffers_clone.write().await;
                        buffers.entry(msg.session_id.clone()).or_insert_with(Vec::new).push(msg);
                    }
                    // 处理 Task 消息
                    Ok(msg) = task_rx.recv() => {
                        // 积累任务消息
                        let mut buffers = task_buffers_clone.write().await;
                        buffers.entry(msg.session_id.clone()).or_insert_with(Vec::new).push(msg);
                    }
                    else => break,
                }
            }
        });

        Self {
            bus,
            storage,
            states,
            agent_buffers,
            notification_buffers,
            task_buffers,
            _store_handle,
        }
    }

    /// 获取消息总线引用 (用于生产者发送消息)
    pub fn bus(&self) -> &MessageBus {
        &self.bus
    }

    /// 订阅 Agent 消息: 返回 (历史消息列表, 实时消息流)
    pub async fn subscribe_agent(
        &self,
        session_id: String,
    ) -> Result<(
        Vec<AgentMessage>,
        Pin<Box<dyn Stream<Item = Result<AgentMessage, broadcast::error::RecvError>> + Send>>,
    )> {
        // 1. 读取历史 Msg 消息
        let history = self.storage.read_agent_messages(&session_id).await?;

        // 2. Flush 积累的 Chunk 消息
        let mut accumulated = Vec::new();
        {
            let mut buffers = self.agent_buffers.write().await;
            let keys_to_remove: Vec<(String, String)> = buffers.keys()
                .filter(|(sess_id, _)| sess_id == &session_id)
                .cloned()
                .collect();
            
            for key in keys_to_remove {
                if let Some(msgs) = buffers.remove(&key) {
                    accumulated.extend(msgs);
                }
            }
        }
        
        // 按 timestamp 排序
        accumulated.sort_by_key(|m| m.timestamp);
        
        // 合并历史和积累的消息
        let mut all_history = history;
        all_history.extend(accumulated);

        // 3. 订阅实时
        let rx = self.bus.subscribe_agent();
        let session_id_clone = session_id.clone();
        
        let stream = BroadcastStream::new(rx)
            .filter_map(move |res| {
                let keep = res.as_ref().map_or(true, |m| m.session_id == session_id_clone);
                std::future::ready(if keep { Some(res) } else { None })
            })
            .map(|item| {
                item.map_err(|e| match e {
                    BroadcastStreamRecvError::Lagged(n) => RecvError::Lagged(n),
                })
            });

        Ok((all_history, Box::pin(stream)))
    }

    /// 订阅通知消息: 返回 (历史消息列表, 实时消息流)
    pub async fn subscribe_notification(
        &self,
        session_id: String,
    ) -> Result<(
        Vec<NotificationMessage>,
        Pin<Box<dyn Stream<Item = Result<NotificationMessage, broadcast::error::RecvError>> + Send>>,
    )> {
        // 1. Flush 积累的通知消息
        let mut accumulated = Vec::new();
        {
            let mut buffers = self.notification_buffers.write().await;
            if let Some(msgs) = buffers.remove(&session_id) {
                accumulated = msgs;
            }
        }
        
        // 按 timestamp 排序
        accumulated.sort_by_key(|m| m.timestamp);

        // 2. 订阅实时
        let rx = self.bus.subscribe_notification();
        let session_id_clone = session_id.clone();
        
        let stream = BroadcastStream::new(rx)
            .filter_map(move |res| {
                let keep = res.as_ref().map_or(true, |m| m.session_id == session_id_clone);
                std::future::ready(if keep { Some(res) } else { None })
            })
            .map(|item| {
                item.map_err(|e| match e {
                    BroadcastStreamRecvError::Lagged(n) => RecvError::Lagged(n),
                })
            });

        Ok((accumulated, Box::pin(stream)))
    }

    /// 订阅任务消息: 返回 (历史消息列表, 实时消息流)
    pub async fn subscribe_task(
        &self,
        session_id: String,
    ) -> Result<(
        Vec<TaskMessage>,
        Pin<Box<dyn Stream<Item = Result<TaskMessage, broadcast::error::RecvError>> + Send>>,
    )> {
        // 1. Flush 积累的任务消息
        let mut accumulated = Vec::new();
        {
            let mut buffers = self.task_buffers.write().await;
            if let Some(msgs) = buffers.remove(&session_id) {
                accumulated = msgs;
            }
        }
        
        // 按 timestamp 排序
        accumulated.sort_by_key(|m| m.timestamp);

        // 2. 订阅实时
        let rx = self.bus.subscribe_task();
        let session_id_clone = session_id.clone();
        
        let stream = BroadcastStream::new(rx)
            .filter_map(move |res| {
                let keep = res.as_ref().map_or(true, |m| m.session_id == session_id_clone);
                std::future::ready(if keep { Some(res) } else { None })
            })
            .map(|item| {
                item.map_err(|e| match e {
                    BroadcastStreamRecvError::Lagged(n) => RecvError::Lagged(n),
                })
            });

        Ok((accumulated, Box::pin(stream)))
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

    /// 获取会话的完整 Agent 消息历史
    pub async fn get_session_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>> {
        self.storage.read_agent_messages(session_id).await
    }
}