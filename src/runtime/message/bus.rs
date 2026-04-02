use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::task::Poll;
use tokio::sync::mpsc;
use crate::runtime::message::Message;

/// 消息总线事件
#[derive(Debug, Clone)]
pub enum BusEvent {
    /// 消息发布事件
    MessagePublished(Message),
    /// 订阅事件
    Subscribed(String, String), // (session_id, subscriber_id)
    /// 取消订阅事件
    Unsubscribed(String, String), // (session_id, subscriber_id)
}

/// 消息订阅者 trait
#[async_trait::async_trait]
pub trait MessageSubscriber: Send + Sync {
    /// 处理接收到的消息
    async fn on_message(&self, message: Message);
    
    /// 获取订阅者 ID
    fn id(&self) -> String;
}

/// 消息总线
#[derive(Debug, Clone)]
pub struct MessageBus {
    /// 主题到订阅者的映射
    subscriptions: Arc<Mutex<HashMap<String, Vec<Arc<dyn MessageSubscriber>>>>>,
    /// 每个订阅者的消息缓冲区
    buffers: Arc<Mutex<HashMap<String, VecDeque<Message>>>>,
    /// 事件通道
    event_tx: Arc<Mutex<Option<mpsc::Sender<BusEvent>>>>,
    event_rx: Arc<Mutex<Option<mpsc::Receiver<BusEvent>>>>,
}

impl MessageBus {
    /// 创建新的消息总线
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            buffers: Arc::new(Mutex::new(HashMap::new())),
            event_tx: Arc::new(Mutex::new(Some(tx))),
            event_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }
    
    /// 发布消息到指定主题
    pub async fn publish(&self, session_id: &str, message: Message) {
        // 克隆消息以避免所有权问题
        let message_clone = message.clone();
        
        // 发送事件
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(BusEvent::MessagePublished(message_clone.clone())).await;
        }
        
        // 向所有订阅者发送消息
        let subscriptions = self.subscriptions.lock().unwrap();
        if let Some(subscribers) = subscriptions.get(session_id) {
            for subscriber in subscribers {
                // 克隆订阅者以避免死锁
                let subscriber_clone = subscriber.clone();
                let msg_clone = message_clone.clone();
                
                // 异步发送消息
                tokio::spawn(async move {
                    subscriber_clone.on_message(msg_clone).await;
                });
                
                // 添加到订阅者的缓冲区
                let mut buffers = self.buffers.lock().unwrap();
                let subscriber_id = subscriber.id();
                buffers.entry(subscriber_id)
                    .or_insert_with(VecDeque::new)
                    .push_back(message_clone.clone());
            }
        }
    }
    
    /// 订阅指定主题
    pub fn subscribe(&self, session_id: &str, subscriber: Arc<dyn MessageSubscriber>) {
        let subscriber_id = subscriber.id();
        
        // 添加到订阅列表
        let mut subscriptions = self.subscriptions.lock().unwrap();
        subscriptions.entry(session_id.to_string())
            .or_insert_with(Vec::new)
            .push(subscriber);
        
        // 发送订阅事件
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.try_send(BusEvent::Subscribed(session_id.to_string(), subscriber_id));
        }
    }
    
    /// 取消订阅
    pub fn unsubscribe(&self, session_id: &str, subscriber_id: &str) {
        let mut subscriptions = self.subscriptions.lock().unwrap();
        if let Some(subscribers) = subscriptions.get_mut(session_id) {
            subscribers.retain(|s| s.id() != subscriber_id);
            if subscribers.is_empty() {
                subscriptions.remove(session_id);
            }
        }
        
        // 发送取消订阅事件
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.try_send(BusEvent::Unsubscribed(session_id.to_string(), subscriber_id.to_string()));
        }
    }
    
    /// 获取订阅者的消息缓冲区
    pub fn get_buffer(&self, subscriber_id: &str) -> Option<VecDeque<Message>> {
        let buffers = self.buffers.lock().unwrap();
        buffers.get(subscriber_id).cloned()
    }
    
    /// 清除订阅者的消息缓冲区
    pub fn clear_buffer(&self, subscriber_id: &str) {
        let mut buffers = self.buffers.lock().unwrap();
        if let Some(buffer) = buffers.get_mut(subscriber_id) {
            buffer.clear();
        }
    }
    
    /// 启动事件处理器
    pub async fn start_event_handler(&self) {
        if let Some(rx) = self.event_rx.lock().unwrap().take() {
            tokio::spawn(async move {
                let mut rx = rx;
                while let Some(event) = rx.recv().await {
                    match event {
                        BusEvent::MessagePublished(message) => {
                            // 可以在这里添加全局消息处理逻辑
                            tracing::debug!("Message published: {:?}", message);
                        }
                        BusEvent::Subscribed(session_id, subscriber_id) => {
                            tracing::debug!("Subscriber {} subscribed to session {}", subscriber_id, session_id);
                        }
                        BusEvent::Unsubscribed(session_id, subscriber_id) => {
                            tracing::debug!("Subscriber {} unsubscribed from session {}", subscriber_id, session_id);
                        }
                    }
                }
            });
        }
    }
}

/// 消息管理器订阅者
#[derive(Debug, Clone)]
pub struct MessageManagerSubscriber {
    id: String,
    message_manager: Arc<super::MessageManager>,
}

impl MessageManagerSubscriber {
    pub fn new(message_manager: Arc<super::MessageManager>) -> Self {
        Self {
            id: format!("message_manager_{}", uuid::Uuid::new_v4()),
            message_manager,
        }
    }
}

#[async_trait::async_trait]
impl MessageSubscriber for MessageManagerSubscriber {
    async fn on_message(&self, message: Message) {
        // 记录消息到存储
        if let Err(e) = self.message_manager.write_message(&message.session_id, message) {
            tracing::error!("Failed to write message: {:?}", e);
        }
    }
    
    fn id(&self) -> String {
        self.id.clone()
    }
}