//! Session Manager 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use crate::bus::MessageBus;
use crate::storage::StorageBackend;
use crate::types::{SessionConfig, SessionState};
use anyhow::Result;
use caelix_api::message::{AgentMessage, AgentMessageType, NotificationMessage, TaskMessage};
use caelix_api::provider::ChatMessage;
use caelix_api::tool::ToolCallApprovalState;
use futures::Stream;
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

// 类型别名，简化复杂类型
type AgentBufferMap = HashMap<(String, String, String), Vec<AgentMessage>>;

pub struct SessionManager {
    bus: MessageBus,
    storage: Arc<dyn StorageBackend>,
    // 内存状态：session_id -> state
    states: Arc<tokio::sync::RwLock<HashMap<String, SessionState>>>,
    // Agent 消息缓冲：(session_id, request_id, span_id) -> Vec<AgentMessage>
    agent_buffers: Arc<tokio::sync::RwLock<AgentBufferMap>>,
    // Notification 消息通道和历史记录
    notification_channels:
        Arc<tokio::sync::RwLock<HashMap<String, mpsc::Sender<NotificationMessage>>>>,
    notification_history: Arc<tokio::sync::RwLock<HashMap<String, VecDeque<NotificationMessage>>>>,
    // Task 消息通道和历史记录
    task_channels: Arc<tokio::sync::RwLock<HashMap<String, mpsc::Sender<TaskMessage>>>>,
    task_history: Arc<tokio::sync::RwLock<HashMap<String, VecDeque<TaskMessage>>>>,
    // 三个独立的消费者任务句柄
    _agent_handle: JoinHandle<()>,
    _notification_handle: JoinHandle<()>,
    _task_handle: JoinHandle<()>,
}

// 缓冲区大小限制
const NOTIFICATION_CHANNEL_CAPACITY: usize = 1000;
const TASK_CHANNEL_CAPACITY: usize = 1000;
const MAX_HISTORY_SIZE: usize = 1000; // 每个 session 保留的历史消息数

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("states", &self.states)
            .finish()
    }
}

// ==================== 独立的消费者函数 ====================

/// Agent 消息消费者（保持现有逻辑）
async fn run_agent_consumer(
    mut rx: broadcast::Receiver<AgentMessage>,
    storage: Arc<dyn StorageBackend>,
    agent_buffers: Arc<tokio::sync::RwLock<AgentBufferMap>>,
) {
    while let Ok(msg) = rx.recv().await {
        match msg.r#type {
            AgentMessageType::Msg => {
                // 持久化 Msg 类型
                if let Err(e) = storage.append_agent_message(&msg).await {
                    tracing::warn!(error = %e, "[Storage] Failed to append agent message");
                }
            }
            AgentMessageType::Event => {
                // 持久化 Event 类型（触发事件标记，供前端在历史中展示时机
                if let Err(e) = storage.append_agent_message(&msg).await {
                    tracing::warn!(error = %e, "[Storage] Failed to append event message");
                }
            }
            AgentMessageType::ManualApproval => {
                // 持久化 ManualApproval：携带审批请求信息
                if let Err(e) = storage.append_agent_message(&msg).await {
                    tracing::warn!(error = %e, "[Storage] Failed to append manual_approval message");
                }
            }
            AgentMessageType::Chunk => {
                // 积累 Chunk 消息 - 使用 (session_id, request_id, span_id) 作为唯一标识
                let key = (
                    msg.session_id.clone(),
                    msg.request_id.clone(),
                    msg.span_id.clone(),
                );
                let mut buffers = agent_buffers.write().await;
                buffers.entry(key).or_insert_with(Vec::new).push(msg);
            }
            AgentMessageType::ChunkEnd => {
                // 清空该 (session_id, request_id, span_id) 的缓冲 - 只清理对应请求的缓冲
                let key = (
                    msg.session_id.clone(),
                    msg.request_id.clone(),
                    msg.span_id.clone(),
                );
                let mut buffers = agent_buffers.write().await;
                buffers.remove(&key);
            }
        }
    }
}

/// Notification 消息消费者（带背压）
async fn run_notification_consumer(
    mut rx: broadcast::Receiver<NotificationMessage>,
    channels: Arc<tokio::sync::RwLock<HashMap<String, mpsc::Sender<NotificationMessage>>>>,
    history: Arc<tokio::sync::RwLock<HashMap<String, VecDeque<NotificationMessage>>>>,
) {
    while let Ok(msg) = rx.recv().await {
        let session_id = msg.session_id.clone();

        // 获取或创建通道
        let sender = {
            let mut channs = channels.write().await;
            channs
                .entry(session_id.clone())
                .or_insert_with(|| {
                    let (tx, _rx) = mpsc::channel(NOTIFICATION_CHANNEL_CAPACITY);
                    tx
                })
                .clone()
        };

        // 尝试发送（如果通道满会阻塞，实现背压）
        if sender.send(msg.clone()).await.is_err() {
            // 接收者已断开，清理通道
            let mut channs = channels.write().await;
            channs.remove(&session_id);
            continue;
        }

        // 同时保存到历史记录（限制大小）
        let mut hist = history.write().await;
        let hist_vec = hist.entry(session_id).or_insert_with(VecDeque::new);
        if hist_vec.len() >= MAX_HISTORY_SIZE {
            hist_vec.pop_front();
        }
        hist_vec.push_back(msg);
    }
}

/// Task 消息消费者（带背压）
async fn run_task_consumer(
    mut rx: broadcast::Receiver<TaskMessage>,
    channels: Arc<tokio::sync::RwLock<HashMap<String, mpsc::Sender<TaskMessage>>>>,
    history: Arc<tokio::sync::RwLock<HashMap<String, VecDeque<TaskMessage>>>>,
) {
    while let Ok(msg) = rx.recv().await {
        let session_id = msg.session_id.clone();

        // 获取或创建通道
        let sender = {
            let mut channs = channels.write().await;
            channs
                .entry(session_id.clone())
                .or_insert_with(|| {
                    let (tx, _rx) = mpsc::channel(TASK_CHANNEL_CAPACITY);
                    tx
                })
                .clone()
        };

        // 尝试发送（如果通道满会阻塞，实现背压）
        if sender.send(msg.clone()).await.is_err() {
            // 接收者已断开，清理通道
            let mut channs = channels.write().await;
            channs.remove(&session_id);
            continue;
        }

        // 同时保存到历史记录（限制大小）
        let mut hist = history.write().await;
        let hist_vec = hist.entry(session_id).or_insert_with(VecDeque::new);
        if hist_vec.len() >= MAX_HISTORY_SIZE {
            hist_vec.pop_front();
        }
        hist_vec.push_back(msg);
    }
}

impl SessionManager {
    pub fn new(bus: MessageBus, storage: Arc<dyn StorageBackend>) -> Self {
        let states = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let agent_buffers: Arc<tokio::sync::RwLock<AgentBufferMap>> =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let notification_channels = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let notification_history = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let task_channels = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let task_history = Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        // 克隆引用用于异步任务
        let agent_buffers_clone = agent_buffers.clone();
        let notif_channels_clone = notification_channels.clone();
        let notif_history_clone = notification_history.clone();
        let task_channels_clone = task_channels.clone();
        let task_history_clone = task_history.clone();
        let storage_clone = storage.clone();

        // 订阅三个通道
        let agent_rx = bus.subscribe_agent();
        let notification_rx = bus.subscribe_notification();
        let task_rx = bus.subscribe_task();

        // 启动三个独立的消费者任务
        // 注意：这些消费者函数不需要 RuntimeContext，因为它们只处理消息分发和持久化
        let _agent_handle = tokio::spawn(async move {
            run_agent_consumer(agent_rx, storage_clone, agent_buffers_clone).await;
        });

        let _notification_handle = tokio::spawn(async move {
            run_notification_consumer(notification_rx, notif_channels_clone, notif_history_clone)
                .await;
        });

        let _task_handle = tokio::spawn(async move {
            run_task_consumer(task_rx, task_channels_clone, task_history_clone).await;
        });

        Self {
            bus,
            storage,
            states,
            agent_buffers,
            notification_channels,
            notification_history,
            task_channels,
            task_history,
            _agent_handle,
            _notification_handle,
            _task_handle,
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
            // 先收集 keys（快速读取）
            let keys_to_remove: Vec<(String, String, String)> = {
                let buffers = self.agent_buffers.read().await;
                buffers
                    .keys()
                    .filter(|(sess_id, _, _)| sess_id == &session_id)
                    .cloned()
                    .collect()
            };

            // 锁已释放，再执行删除
            let mut buffers = self.agent_buffers.write().await;
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
                let keep = res
                    .as_ref()
                    .map_or(true, |m| m.session_id == session_id_clone);
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
        Pin<
            Box<dyn Stream<Item = Result<NotificationMessage, broadcast::error::RecvError>> + Send>,
        >,
    )> {
        // 1. 从历史记录中获取累积消息
        let accumulated = {
            let mut hist = self.notification_history.write().await;
            hist.remove(&session_id)
                .map(|deque| deque.into_iter().collect::<Vec<_>>())
                .unwrap_or_default()
        };

        // 2. 创建新的 mpsc 通道供订阅者使用
        let (tx, rx) = mpsc::channel(NOTIFICATION_CHANNEL_CAPACITY);

        // 3. 注册通道
        {
            let mut channels = self.notification_channels.write().await;
            channels.insert(session_id.clone(), tx);
        }

        // 4. 将 mpsc::Receiver 转换为 Stream
        let stream = ReceiverStream::new(rx).map(Ok::<_, RecvError>); // 转换为 Result 类型

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
        // 1. 从历史记录中获取累积消息
        let accumulated = {
            let mut hist = self.task_history.write().await;
            hist.remove(&session_id)
                .map(|deque| deque.into_iter().collect::<Vec<_>>())
                .unwrap_or_default()
        };

        // 2. 创建新的 mpsc 通道供订阅者使用
        let (tx, rx) = mpsc::channel(TASK_CHANNEL_CAPACITY);

        // 3. 注册通道
        {
            let mut channels = self.task_channels.write().await;
            channels.insert(session_id.clone(), tx);
        }

        // 4. 将 mpsc::Receiver 转换为 Stream
        let stream = ReceiverStream::new(rx).map(Ok::<_, RecvError>); // 转换为 Result 类型

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
        states
            .get(session_id)
            .and_then(|state| state.config.clone())
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

    /// 获取 agent_buffers 引用（用于信号处理）
    pub fn get_agent_buffers(&self) -> &Arc<tokio::sync::RwLock<AgentBufferMap>> {
        &self.agent_buffers
    }

    /// 获取 storage 引用（用于信号处理）
    pub fn get_storage(&self) -> &Arc<dyn StorageBackend> {
        &self.storage
    }

    /// 为指定 tool_call_id 审批一条 Assistant Msg 中的 tool_call（含 approval_state）。
    /// 从历史中从后向前查找，找到第一个 role=assistant 且包含该 tool_call_id 的 Msg，
    /// 然后在 tool_calls 中对应项设置 approval_state，并写回存储。
    ///
    /// 返回值：`Ok(Some(index))` 表示找到并替换；`Ok(None)` 未找到；`Err` 存储错误。
    pub async fn update_tool_approval(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<Option<(usize, AgentMessage)>> {
        let messages = self.storage.read_agent_messages(session_id).await?;

        // 从后向前搜索
        for (idx, msg) in messages.iter().enumerate().rev() {
            if msg.r#type != AgentMessageType::Msg {
                continue;
            }
            // 尝试反序列化为 ChatMessage
            if let Ok(mut chat_msg) = serde_json::from_str::<ChatMessage>(&msg.content) {
                if chat_msg.role != "assistant" {
                    continue;
                }
                let tool_calls = match chat_msg.tool_calls.as_mut() {
                    Some(tcs) => tcs,
                    None => continue,
                };
                let mut found = false;
                for tc in tool_calls.iter_mut() {
                    if tc.id == tool_call_id {
                        tc.approval_state = if approved {
                            Some(ToolCallApprovalState::Approved)
                        } else {
                            Some(ToolCallApprovalState::Rejected)
                        };
                        found = true;
                        break;
                    }
                }
                if !found {
                    continue;
                }
                // 构造新的 AgentMessage（保持其他字段不变，仅替换 content 为新 JSON）
                let new_content = serde_json::to_string(&chat_msg)?;
                let new_agent_msg = AgentMessage {
                    session_id: msg.session_id.clone(),
                    request_id: msg.request_id.clone(),
                    span_id: msg.span_id.clone(),
                    trace_id: msg.trace_id.clone(),
                    r#type: msg.r#type.clone(),
                    timestamp: msg.timestamp,
                    content: new_content,
                    agent_name: msg.agent_name.clone(),
                    usage: msg.usage.clone(),
                };
                // 写回存储
                self.storage
                    .replace_agent_message(session_id, idx, &new_agent_msg)
                    .await?;
                return Ok(Some((idx, new_agent_msg)));
            }
        }
        Ok(None)
    }

    /// 刷新指定 session 的所有待持久化消息
    ///
    /// 注意：agent_buffers 中只存储 Chunk 消息（不持久化），
    /// Msg 消息已经在消费者中实时持久化了。
    /// 此方法主要用于确保所有异步操作完成。
    pub async fn flush_session(&self, _session_id: &str) {
        // 等待一小段时间，确保消费者完成当前的持久化操作
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 这里可以添加更多的同步逻辑，比如：
        // 1. 检查是否有正在进行的持久化操作
        // 2. 等待所有 pending 的写操作完成
        // 目前由于 Msg 是实时持久化的，只需要短暂等待即可
    }
}

// 实现 caelix-api 中定义的 SessionManagerTrait
#[async_trait::async_trait]
impl caelix_api::message::SessionManagerTrait for SessionManager {
    async fn subscribe_agent(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<caelix_api::message::AgentMessage>,
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<caelix_api::message::AgentMessage, String>>
                        + Send,
                >,
            >,
        ),
        String,
    > {
        let (history, stream) = self
            .subscribe_agent(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let mapped = stream.map(|item| item.map_err(|e| e.to_string()));
        Ok((history, Box::pin(mapped)))
    }

    async fn subscribe_notification(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<caelix_api::message::NotificationMessage>,
            std::pin::Pin<
                Box<
                    dyn futures::Stream<
                            Item = Result<caelix_api::message::NotificationMessage, String>,
                        > + Send,
                >,
            >,
        ),
        String,
    > {
        let (history, stream) = self
            .subscribe_notification(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let mapped = stream.map(|item| item.map_err(|e| e.to_string()));
        Ok((history, Box::pin(mapped)))
    }

    async fn subscribe_task(
        &self,
        session_id: String,
    ) -> Result<
        (
            Vec<caelix_api::message::TaskMessage>,
            std::pin::Pin<
                Box<
                    dyn futures::Stream<Item = Result<caelix_api::message::TaskMessage, String>>
                        + Send,
                >,
            >,
        ),
        String,
    > {
        let (history, stream) = self
            .subscribe_task(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let mapped = stream.map(|item| item.map_err(|e| e.to_string()));
        Ok((history, Box::pin(mapped)))
    }

    async fn get_session_state(&self, session_id: &str) -> String {
        format!("{:?}", self.states.read().await.get(session_id).cloned())
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        self.states.read().await.contains_key(session_id)
    }

    async fn list_sessions(&self) -> Vec<String> {
        self.states.read().await.keys().cloned().collect()
    }

    fn bus(&self) -> std::sync::Arc<dyn caelix_api::message::MessageBusTrait> {
        Arc::new(self.bus.clone())
    }
}
