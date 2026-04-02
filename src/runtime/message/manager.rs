use std::fs::{self, File, OpenOptions};
use std::io::{self, Write, BufReader, BufRead};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::Utc;
use crate::core::llm::Message as LlmMessage;
use crate::core::llm::MessageRole;
use super::MessageBus;
use super::MessageSubscriber;
use super::MessageManagerSubscriber; 

/// 消息结构体，用于消息管理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<String>, // 简化处理，实际应该使用ToolCall类型
    pub timestamp: i64,
    pub session_id: String,
    pub belongs_to: Option<String>, // 所属，可以是任务ID或其他会话ID
}

/// 消息管理器，以SessionId维度管理消息
#[derive(Debug, Clone)]
pub struct MessageManager {
    base_dir: PathBuf,
    message_bus: Option<Arc<MessageBus>>,
    subscriber: Option<Arc<MessageManagerSubscriber>>,
}

impl MessageManager {
    /// 创建新的消息管理器
    pub fn new() -> Result<Self, io::Error> {
        // 获取用户主目录
        let home_dir = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "无法获取用户主目录"))?;
        // 创建 ~/.caelix 目录
        let base_dir = home_dir.join(".caelix");
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)?;
        }
        Ok(Self { 
            base_dir, 
            message_bus: None, 
            subscriber: None 
        })
    }
    
    /// 注册到消息总线
    pub fn register_to_bus(&mut self, message_bus: Arc<MessageBus>) {
        let subscriber = Arc::new(MessageManagerSubscriber::new(Arc::new(self.clone())));
        self.message_bus = Some(message_bus.clone());
        self.subscriber = Some(subscriber.clone());
        
        // 这里可以订阅所有会话，或者根据需要订阅特定会话
        // 暂时不自动订阅，由外部调用subscribe方法
    }
    
    /// 订阅特定会话的消息
    pub fn subscribe(&self, session_id: &str) {
        if let (Some(bus), Some(subscriber)) = (&self.message_bus, &self.subscriber) {
            bus.subscribe(session_id, subscriber.clone());
        }
    }

    /// 创建新的会话ID
    pub fn create_session(&self) -> String {
        // 生成UUID作为会话ID
        let session_id = Uuid::new_v4().to_string();
        // 创建对应会话目录
        let session_dir = self.base_dir.join(&session_id);
        if !session_dir.exists() {
            let _ = fs::create_dir_all(&session_dir);
        }
        session_id
    }

    /// 写入消息到指定会话
    pub fn write_message(&self, session_id: &str, message: Message) -> Result<(), io::Error> {
        // 确保会话目录存在
        let session_dir = self.base_dir.join(session_id);
        if !session_dir.exists() {
            fs::create_dir_all(&session_dir)?;
        }
        
        // 构建消息文件路径
        let messages_file = session_dir.join("messages.jsonl");
        
        // 打开文件，追加模式
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&messages_file)?;
        
        // 序列化消息为JSON并写入文件
        let message_json = serde_json::to_string(&message)?;
        writeln!(file, "{}")?;
        
        Ok(())
    }

    /// 根据会话ID获取所有历史消息
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<Message>, io::Error> {
        // 构建消息文件路径
        let session_dir = self.base_dir.join(session_id);
        let messages_file = session_dir.join("messages.jsonl");
        
        // 检查文件是否存在
        if !messages_file.exists() {
            return Ok(vec![]);
        }
        
        // 读取文件内容
        let file = File::open(&messages_file)?;
        let reader = BufReader::new(file);
        
        // 解析每一行的JSON为消息
        let mut messages = vec![];
        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                let message: Message = serde_json::from_str(&line)?;
                messages.push(message);
            }
        }
        
        Ok(messages)
    }

    /// 从LLM消息创建管理消息
    pub fn from_llm_message(&self, session_id: &str, llm_message: LlmMessage, belongs_to: Option<String>) -> Message {
        Message {
            id: Uuid::new_v4().to_string(),
            role: llm_message.role,
            content: llm_message.content,
            tool_calls: vec![], // 简化处理
            timestamp: Utc::now().timestamp(),
            session_id: session_id.to_string(),
            belongs_to,
        }
    }
    
    /// 发布消息到总线
    pub async fn publish_message(&self, message: Message) {
        if let Some(bus) = &self.message_bus {
            bus.publish(&message.session_id, message).await;
        }
    }
}