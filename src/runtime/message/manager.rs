use crate::runtime::message::bus::MessageBus;
use crate::runtime::message::storage::StorageBackend;
use crate::runtime::message::types::{ActiveSpanInfo, Message, SessionState, Status};
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
}