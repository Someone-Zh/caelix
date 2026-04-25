//! Storage 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use crate::runtime::message::agent_message::AgentMessage;
use crate::runtime::message::types::SessionState;
use anyhow::Result;
use async_trait::async_trait;
use serde_json;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// 存储后端 Trait (未来可实现 DbStorage)
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// 追加单条 Agent 消息
    async fn append_agent_message(&self, msg: &AgentMessage) -> Result<()>;
    
    /// 读取 Session 的全部历史 Agent 消息
    async fn read_agent_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>>;
    
    /// 保存 Session 状态快照
    async fn save_state(&self, session_id: &str, state: &SessionState) -> Result<()>;
    
    /// 加载 Session 状态快照
    async fn load_state(&self, session_id: &str) -> Result<Option<SessionState>>;
}

/// 文件系统存储实现
pub struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn get_session_dir(&self, session_id: &str) -> PathBuf {
        self.base_path.join(session_id)
    }

    fn get_agent_messages_path(&self, session_id: &str) -> PathBuf {
        self.get_session_dir(session_id).join("agent_messages.jsonl")
    }

    fn get_state_path(&self, session_id: &str) -> PathBuf {
        self.get_session_dir(session_id).join("state.json")
    }

    fn get_wal_path(&self, session_id: &str) -> PathBuf {
        self.get_session_dir(session_id).join("pending.log")
    }



    async fn ensure_session_dir(&self, session_id: &str) -> Result<()> {
        let dir = self.get_session_dir(session_id);
        if !dir.exists() {
            fs::create_dir_all(&dir).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn append_agent_message(&self, msg: &AgentMessage) -> Result<()> {
        self.ensure_session_dir(&msg.session_id).await?;
        
        // 1. 写入 WAL (Write-Ahead Log)
        let wal_path = self.get_wal_path(&msg.session_id);
        let mut wal_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .await?;
        let wal_line = serde_json::to_string(msg)? + "\n";
        wal_file.write_all(wal_line.as_bytes()).await?;
        wal_file.flush().await?;
        wal_file.sync_data().await?;

        // 2. 追加到主文件 (原子写入：Tmp -> Rename)
        let msg_path = self.get_agent_messages_path(&msg.session_id);
        let tmp_path = msg_path.with_extension("jsonl.tmp");

        // 如果主文件存在，先复制到 tmp
        if msg_path.exists() {
            fs::copy(&msg_path, &tmp_path).await?;
        }

        // 追加新行
        let mut tmp_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp_path)
            .await?;
        let line = serde_json::to_string(msg)? + "\n";
        tmp_file.write_all(line.as_bytes()).await?;
        tmp_file.flush().await?;
        tmp_file.sync_data().await?;

        // 原子重命名
        fs::rename(tmp_path, msg_path).await?;

        // 3. 清除 WAL
        fs::remove_file(wal_path).await?;

        Ok(())
    }

    async fn read_agent_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>> {
        let path = self.get_agent_messages_path(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(path).await?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let msg: AgentMessage = serde_json::from_str(&line)?;
            messages.push(msg);
        }

        // 按 timestamp 排序
        messages.sort_by_key(|m| m.timestamp);
        Ok(messages)
    }

    async fn save_state(&self, session_id: &str, state: &SessionState) -> Result<()> {
        self.ensure_session_dir(session_id).await?;
        let path = self.get_state_path(session_id);
        let tmp_path = path.with_extension("json.tmp");
        
        let json = serde_json::to_string_pretty(state)?;
        fs::write(&tmp_path, json).await?;
        fs::rename(tmp_path, path).await?;
        Ok(())
    }

    async fn load_state(&self, session_id: &str) -> Result<Option<SessionState>> {
        let path = self.get_state_path(session_id);
        if !path.exists() {
            return Ok(None);
        }
        let json = fs::read_to_string(path).await?;
        let state: SessionState = serde_json::from_str(&json)?;
        Ok(Some(state))
    }


}