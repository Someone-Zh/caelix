use crate::runtime::message::types::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use std::sync::Arc; 

#[derive(Debug, Clone)]
pub struct MessageBus {
    sender: broadcast::Sender<Message>,
    seq_counter: Arc<AtomicU64>, 

}

impl MessageBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            seq_counter: Arc::new(AtomicU64::new(1)), 
        }
    }

    /// 发送消息 (自动分配全局 Seq)
    pub fn send(&self, mut msg: Message) -> Result<(), broadcast::error::SendError<Message>> {
        let seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
        msg.seq = seq;
        self.sender.send(msg)?;
        Ok(())
    }

    /// 订阅消息
    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.sender.subscribe()
    }
}