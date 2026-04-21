// src/runtime/task/persistence.rs
use crate::runtime::task::types::TaskMeta;
use anyhow::Result;
use async_trait::async_trait;
use serde_json;
use std::path::PathBuf;
use tokio::fs;

#[async_trait]
pub trait TaskPersistence: Send + Sync + 'static {
    async fn save(&self, meta: &TaskMeta) -> Result<()>;
    async fn delete(&self, task_id: &str) -> Result<()>;
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

    fn get_task_path(&self, task_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", task_id))
    }

    async fn ensure_dir(&self) -> Result<()> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl TaskPersistence for FilePersistence {
    async fn save(&self, meta: &TaskMeta) -> Result<()> {
        // 只有定时和周期任务需要持久化
        match meta.kind {
            crate::runtime::task::types::TaskKind::Async => return Ok(()),
            _ => {}
        }

        self.ensure_dir().await?;
        let path = self.get_task_path(&meta.task_id.to_string());
        let json = serde_json::to_string_pretty(meta)?;
        fs::write(path, json).await?;
        Ok(())
    }

    async fn delete(&self, task_id: &str) -> Result<()> {
        let path = self.get_task_path(task_id);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn load_all(&self) -> Result<Vec<TaskMeta>> {
        self.ensure_dir().await?;
        let mut tasks = Vec::new();

        let mut dir = match fs::read_dir(&self.base_path).await {
            Ok(d) => d,
            Err(_) => return Ok(tasks),
        };

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(path).await?;
                if let Ok(meta) = serde_json::from_str::<TaskMeta>(&content) {
                    tasks.push(meta);
                }
            }
        }

        Ok(tasks)
    }
}