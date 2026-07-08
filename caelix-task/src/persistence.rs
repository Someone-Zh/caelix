// src/runtime/task/persistence.rs
use crate::types::TaskMeta;
use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;

#[async_trait]
pub trait TaskPersistence: Send + Sync + 'static {
    async fn save(&self, meta: &TaskMeta) -> Result<()>;
    async fn delete(&self, session_id: &str, task_id: &str) -> Result<()>;
    async fn load_all(&self) -> Result<Vec<TaskMeta>>;
}

pub struct FilePersistence {
    base_path: PathBuf,
}

impl FilePersistence {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn validate_path_id(kind: &str, id: &str) -> Result<()> {
        if !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            Ok(())
        } else {
            anyhow::bail!("Invalid {}: only [A-Za-z0-9_-] is allowed", kind);
        }
    }

    /// 获取 session 级别的任务存储路径
    fn get_session_task_path(&self, session_id: &str, task_id: &str) -> Result<PathBuf> {
        Self::validate_path_id("session_id", session_id)?;
        Self::validate_path_id("task_id", task_id)?;
        Ok(self
            .base_path
            .join(session_id)
            .join(format!("{}.json", task_id)))
    }

    async fn ensure_dir(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }

    async fn ensure_session_dir(&self, session_id: &str) -> Result<()> {
        Self::validate_path_id("session_id", session_id)?;
        let session_path = self.base_path.join(session_id);
        if !session_path.exists() {
            fs::create_dir_all(&session_path).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl TaskPersistence for FilePersistence {
    async fn save(&self, meta: &TaskMeta) -> Result<()> {
        // 所有任务都需要持久化，包括 Async 任务
        // 使用 session 级别的路径组织任务文件
        self.ensure_session_dir(&meta.session_id).await?;
        let path = self.get_session_task_path(&meta.session_id, &meta.task_id.to_string())?;
        let json = serde_json::to_string_pretty(meta)?;
        fs::write(path, json).await?;
        Ok(())
    }

    async fn delete(&self, session_id: &str, task_id: &str) -> Result<()> {
        let path = self.get_session_task_path(session_id, task_id)?;
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn load_all(&self) -> Result<Vec<TaskMeta>> {
        self.ensure_dir().await?;
        let mut tasks = Vec::new();

        // 遍历所有 session 目录
        let mut dir = match fs::read_dir(&self.base_path).await {
            Ok(d) => d,
            Err(_) => return Ok(tasks),
        };

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            // 如果是目录（session 目录），则遍历其中的任务文件
            if path.is_dir() {
                let mut session_dir = fs::read_dir(&path).await?;
                while let Some(session_entry) = session_dir.next_entry().await? {
                    let task_path = session_entry.path();
                    if task_path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let content = fs::read_to_string(&task_path).await?;
                        if let Ok(meta) = serde_json::from_str::<TaskMeta>(&content) {
                            tasks.push(meta);
                        }
                    }
                }
            }
        }

        Ok(tasks)
    }
}
