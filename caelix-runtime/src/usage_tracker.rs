//! Token 用量追踪器
//!
//! 负责：
//! - 持久化：向 `$CAELIX_HOME/statistics/usage.jsonl` 追加 JSON Lines 格式的用量记录
//! - 内存聚合：以 session_id、(provider, model) 维度汇总，提供查询接口
//! - 启动恢复：从磁盘重新加载历史记录

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use caelix_api::context::UsageTrackerTrait;
use caelix_api::provider::{
    GlobalUsageView, ProviderUsageView, SessionUsageView, UsageRecord, UsageSnapshot,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

#[derive(Default)]
struct Aggregates {
    by_session: std::collections::HashMap<String, UsageSnapshot>,
    by_provider_model: std::collections::HashMap<(String, String), UsageSnapshot>,
    total: UsageSnapshot,
}

/// Token 用量追踪器（具体实现）
pub struct UsageTracker {
    file_path: PathBuf,
    file_lock: Mutex<()>,
    agg: RwLock<Aggregates>,
}

impl UsageTracker {
    pub fn new(caelix_home: &Path) -> Self {
        let dir = caelix_home.join("statistics");
        // 初始化时尝试创建目录；失败时静默，后续写入时再抛错
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("usage.jsonl");

        let tracker = Self {
            file_path,
            file_lock: Mutex::new(()),
            agg: RwLock::new(Aggregates::default()),
        };

        // 同步从磁盘恢复（程序启动时调用一次即可，IO 很小）
        if let Err(e) = tracker.reload_from_disk_sync() {
            tracing::warn!(error = %e, "从 usage.jsonl 恢复历史用量失败，将从空开始");
        }

        tracker
    }

    /// 同步方式：将 usage.jsonl 中的全部记录重放到内存聚合
    fn reload_from_disk_sync(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file = match File::open(&self.file_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let reader = BufReader::new(file);
        let mut agg = Aggregates::default();
        for (i, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!(line = i, error = %e, "读取 usage.jsonl 行失败，跳过");
                    continue;
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            let record: UsageRecord = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(line = i, error = %e, "usage.jsonl 行解析失败，跳过");
                    continue;
                }
            };
            apply_record(&mut agg, &record);
        }
        *self.agg.blocking_write() = agg;
        Ok(())
    }
}

fn apply_record(agg: &mut Aggregates, record: &UsageRecord) {
    agg.total.prompt_tokens = agg.total.prompt_tokens.saturating_add(record.prompt_tokens);
    agg.total.completion_tokens = agg
        .total
        .completion_tokens
        .saturating_add(record.completion_tokens);
    agg.total.total_tokens = agg.total.total_tokens.saturating_add(record.total_tokens);
    if let Some(r) = record.reasoning_tokens {
        agg.total.reasoning_tokens = agg.total.reasoning_tokens.saturating_add(r);
    }
    if let Some(c) = record.cache_hit_tokens {
        agg.total.cache_hit_tokens = agg.total.cache_hit_tokens.saturating_add(c);
    }
    agg.total.record_count = agg.total.record_count.saturating_add(1);
    if agg.total.first_timestamp.is_none() {
        agg.total.first_timestamp = Some(record.timestamp.clone());
    }
    agg.total.last_timestamp = Some(record.timestamp.clone());

    // by_session
    let snap = agg.by_session.entry(record.session_id.clone()).or_default();
    snap.prompt_tokens = snap.prompt_tokens.saturating_add(record.prompt_tokens);
    snap.completion_tokens = snap
        .completion_tokens
        .saturating_add(record.completion_tokens);
    snap.total_tokens = snap.total_tokens.saturating_add(record.total_tokens);
    if let Some(r) = record.reasoning_tokens {
        snap.reasoning_tokens = snap.reasoning_tokens.saturating_add(r);
    }
    if let Some(c) = record.cache_hit_tokens {
        snap.cache_hit_tokens = snap.cache_hit_tokens.saturating_add(c);
    }
    snap.record_count = snap.record_count.saturating_add(1);
    if snap.first_timestamp.is_none() {
        snap.first_timestamp = Some(record.timestamp.clone());
    }
    snap.last_timestamp = Some(record.timestamp.clone());

    // by_provider_model
    let key = (record.provider.clone(), record.model.clone());
    let snap = agg.by_provider_model.entry(key).or_default();
    snap.prompt_tokens = snap.prompt_tokens.saturating_add(record.prompt_tokens);
    snap.completion_tokens = snap
        .completion_tokens
        .saturating_add(record.completion_tokens);
    snap.total_tokens = snap.total_tokens.saturating_add(record.total_tokens);
    if let Some(r) = record.reasoning_tokens {
        snap.reasoning_tokens = snap.reasoning_tokens.saturating_add(r);
    }
    if let Some(c) = record.cache_hit_tokens {
        snap.cache_hit_tokens = snap.cache_hit_tokens.saturating_add(c);
    }
    snap.record_count = snap.record_count.saturating_add(1);
    if snap.first_timestamp.is_none() {
        snap.first_timestamp = Some(record.timestamp.clone());
    }
    snap.last_timestamp = Some(record.timestamp.clone());
}

#[async_trait::async_trait]
impl UsageTrackerTrait for UsageTracker {
    async fn accumulate(&self, record: UsageRecord) {
        // 1. 写入内存聚合
        {
            let mut agg = self.agg.write().await;
            apply_record(&mut agg, &record);
        }

        // 2. 写入磁盘（JSON Lines append）
        let _guard = self.file_lock.lock().await;
        match append_record_to_disk(&self.file_path, &record).await {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!(error = %e, "写入 usage.jsonl 失败");
            }
        }
    }

    async fn snapshot_session(
        &self,
        session_id: &str,
        ctx_window_tokens: Option<u32>,
    ) -> Option<SessionUsageView> {
        let agg = self.agg.read().await;
        let snap = agg.by_session.get(session_id)?.clone();
        Some(SessionUsageView {
            session_id: session_id.to_string(),
            snapshot: snap.clone(),
            context_size_tokens: snap.prompt_tokens,
            ctx_window_tokens,
        })
    }

    async fn snapshot_global(&self) -> GlobalUsageView {
        let agg = self.agg.read().await;
        GlobalUsageView {
            total: agg.total.clone(),
            by_provider_model: agg
                .by_provider_model
                .iter()
                .map(|((provider, model), snap)| ProviderUsageView {
                    provider: provider.clone(),
                    model: model.clone(),
                    snapshot: snap.clone(),
                })
                .collect(),
            by_session: agg
                .by_session
                .iter()
                .map(|(session_id, snap)| SessionUsageView {
                    session_id: session_id.clone(),
                    snapshot: snap.clone(),
                    context_size_tokens: snap.prompt_tokens,
                    ctx_window_tokens: None,
                })
                .collect(),
        }
    }
}

async fn append_record_to_disk(
    path: &Path,
    record: &UsageRecord,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let line = serde_json::to_string(record)?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    file.flush().await?;
    Ok(())
}
