//! Storage 模块
#![allow(dead_code)] // 部分API为将来扩展预留

use crate::types::SessionState;
use anyhow::Result;
use async_trait::async_trait;
use caelix_api::message::AgentMessage;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// 存储后端 Trait (未来可实现 DbStorage)
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// 追加单条 Agent 消息
    async fn append_agent_message(&self, msg: &AgentMessage) -> Result<()>;

    /// 读取 Session 的全部历史 Agent 消息
    async fn read_agent_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>>;

    /// 替换 Session 中指定 index 的 Agent 消息（0-based）
    /// 若 index 超出范围则将消息追加到末尾。
    async fn replace_agent_message(
        &self,
        session_id: &str,
        index: usize,
        new_msg: &AgentMessage,
    ) -> Result<()>;

    /// 保存 Session 状态快照
    async fn save_state(&self, session_id: &str, state: &SessionState) -> Result<()>;

    /// 加载 Session 状态快照
    async fn load_state(&self, session_id: &str) -> Result<Option<SessionState>>;
}

/// 文件系统存储实现
pub struct FileStorage {
    base_path: PathBuf,
    session_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl FileStorage {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            session_locks: DashMap::new(),
        }
    }

    fn validate_session_id(session_id: &str) -> Result<()> {
        if !session_id.is_empty()
            && session_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            Ok(())
        } else {
            anyhow::bail!("Invalid session_id: only [A-Za-z0-9_-] is allowed");
        }
    }

    fn get_session_dir(&self, session_id: &str) -> Result<PathBuf> {
        Self::validate_session_id(session_id)?;
        Ok(self.base_path.join(session_id))
    }

    fn get_agent_messages_path(&self, session_id: &str) -> Result<PathBuf> {
        Ok(self
            .get_session_dir(session_id)?
            .join("agent_messages.jsonl"))
    }

    fn get_state_path(&self, session_id: &str) -> Result<PathBuf> {
        Ok(self.get_session_dir(session_id)?.join("state.json"))
    }

    fn get_wal_path(&self, session_id: &str) -> Result<PathBuf> {
        Ok(self.get_session_dir(session_id)?.join("pending.log"))
    }

    async fn ensure_session_dir(&self, session_id: &str) -> Result<()> {
        let dir = self.get_session_dir(session_id)?;
        if !dir.exists() {
            fs::create_dir_all(&dir).await?;
        }
        Ok(())
    }

    fn session_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        self.session_locks
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn replay_pending_wal(&self, session_id: &str) -> Result<()> {
        let wal_path = self.get_wal_path(session_id)?;
        if !wal_path.exists() {
            return Ok(());
        }

        let pending = fs::read_to_string(&wal_path).await?;
        let pending_lines = pending
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        if pending_lines.is_empty() {
            let _ = fs::remove_file(&wal_path).await;
            return Ok(());
        }

        let msg_path = self.get_agent_messages_path(session_id)?;
        let existing = if msg_path.exists() {
            fs::read_to_string(&msg_path).await?
        } else {
            String::new()
        };

        let mut existing_lines = existing.lines().collect::<Vec<_>>();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&msg_path)
            .await?;

        for line in pending_lines {
            serde_json::from_str::<AgentMessage>(line)?;
            if existing_lines.iter().any(|existing| *existing == line) {
                continue;
            }
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
            existing_lines.push(line);
        }

        file.flush().await?;
        file.sync_data().await?;
        match fs::remove_file(&wal_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caelix_api::message::AgentMessageType;
    use chrono::Utc;
    use std::sync::Arc;
    use uuid::Uuid;

    fn temp_storage() -> (Arc<FileStorage>, PathBuf) {
        let path = std::env::temp_dir().join(format!("caelix-message-test-{}", Uuid::new_v4()));
        (Arc::new(FileStorage::new(&path)), path)
    }

    fn agent_msg(session_id: &str, content: &str) -> AgentMessage {
        AgentMessage {
            session_id: session_id.to_string(),
            request_id: Uuid::new_v4().to_string(),
            span_id: "span".to_string(),
            trace_id: String::new(),
            r#type: AgentMessageType::Msg,
            timestamp: Utc::now(),
            content: content.to_string(),
            agent_name: None,
            usage: None,
        }
    }

    #[tokio::test]
    async fn concurrent_appends_preserve_all_messages() {
        let (storage, path) = temp_storage();
        let session_id = "session";
        let mut handles = Vec::new();

        for i in 0..50 {
            let storage = storage.clone();
            handles.push(tokio::spawn(async move {
                storage
                    .append_agent_message(&agent_msg(session_id, &format!("msg-{i}")))
                    .await
                    .unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let messages = storage.read_agent_messages(session_id).await.unwrap();
        assert_eq!(messages.len(), 50);

        let _ = fs::remove_dir_all(path).await;
    }

    #[tokio::test]
    async fn pending_wal_is_replayed_on_read() {
        let (storage, path) = temp_storage();
        let session_id = "session";
        storage.ensure_session_dir(session_id).await.unwrap();

        let msg = agent_msg(session_id, "from-wal");
        let wal_path = storage.get_wal_path(session_id).unwrap();
        fs::write(&wal_path, serde_json::to_string(&msg).unwrap() + "\n")
            .await
            .unwrap();

        let messages = storage.read_agent_messages(session_id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "from-wal");
        assert!(!wal_path.exists());

        let _ = fs::remove_dir_all(path).await;
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn append_agent_message(&self, msg: &AgentMessage) -> Result<()> {
        let lock = self.session_lock(&msg.session_id);
        let _guard = lock.lock().await;
        self.ensure_session_dir(&msg.session_id).await?;
        self.replay_pending_wal(&msg.session_id).await?;

        // 1. 写入 WAL (Write-Ahead Log)
        let wal_path = self.get_wal_path(&msg.session_id)?;
        let mut wal_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .await?;
        let wal_line = serde_json::to_string(msg)? + "\n";
        wal_file.write_all(wal_line.as_bytes()).await?;
        wal_file.flush().await?;
        wal_file.sync_data().await?;

        // 2. 追加到主文件
        let msg_path = self.get_agent_messages_path(&msg.session_id)?;
        let mut msg_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&msg_path)
            .await?;
        let line = serde_json::to_string(msg)? + "\n";
        msg_file.write_all(line.as_bytes()).await?;
        msg_file.flush().await?;
        msg_file.sync_data().await?;

        // 3. 清除 WAL
        match fs::remove_file(wal_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    async fn read_agent_messages(&self, session_id: &str) -> Result<Vec<AgentMessage>> {
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        self.ensure_session_dir(session_id).await?;
        self.replay_pending_wal(session_id).await?;

        let path = self.get_agent_messages_path(session_id)?;
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
        let path = self.get_state_path(session_id)?;
        let tmp_path = path.with_extension("json.tmp");

        let json = serde_json::to_string_pretty(state)?;
        fs::write(&tmp_path, json).await?;
        fs::rename(tmp_path, path).await?;
        Ok(())
    }

    async fn load_state(&self, session_id: &str) -> Result<Option<SessionState>> {
        let path = self.get_state_path(session_id)?;
        if !path.exists() {
            return Ok(None);
        }
        let json = fs::read_to_string(path).await?;
        let state: SessionState = serde_json::from_str(&json)?;
        Ok(Some(state))
    }

    async fn replace_agent_message(
        &self,
        session_id: &str,
        index: usize,
        new_msg: &AgentMessage,
    ) -> Result<()> {
        let lock = self.session_lock(session_id);
        let _guard = lock.lock().await;
        self.ensure_session_dir(session_id).await?;
        self.replay_pending_wal(session_id).await?;

        let msg_path = self.get_agent_messages_path(session_id)?;
        let tmp_path = msg_path.with_extension("jsonl.tmp");

        // 1. 读取现有行
        let mut lines: Vec<String> = if msg_path.exists() {
            let file = fs::File::open(&msg_path).await?;
            let reader = BufReader::new(file);
            let mut out = Vec::new();
            let mut iter = reader.lines();
            while let Some(line) = iter.next_line().await? {
                out.push(line);
            }
            out
        } else {
            Vec::new()
        };

        // 2. 替换或追加
        if index >= lines.len() {
            lines.push(serde_json::to_string(new_msg)?);
        } else {
            lines[index] = serde_json::to_string(new_msg)?;
        }

        // 3. 按 timestamp 排序（可选，保持顺序稳定）
        //    由于替换了单条消息，时间戳可能不影响顺序，这里保持原顺序不动。

        // 4. 原子写入 tmp -> rename
        let content = lines
            .into_iter()
            .map(|l| if l.ends_with('\n') { l } else { l + "\n" })
            .collect::<Vec<_>>()
            .join("");
        fs::write(&tmp_path, content).await?;
        fs::rename(tmp_path, msg_path).await?;
        Ok(())
    }
}
