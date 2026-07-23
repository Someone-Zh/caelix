//! 记忆包管理业务逻辑

use crate::types::{
    MemoryAxiom, MemoryBudgetInfo, MemoryCandidate, MemoryConflict, MemoryRecallResult,
    MemoryStats,
};
use caelix_api::error::ApiError;
use caelix_memory::schema::RawSource;
use caelix_memory::MemoryVault;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MemoryService {
    vault: Arc<Mutex<Option<Arc<MemoryVault>>>>,
}

impl MemoryService {
    pub fn new() -> Self {
        Self {
            vault: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_vault(&self) -> Result<Arc<MemoryVault>, ApiError> {
        let mut guard = self.vault.lock().await;
        if guard.is_none() {
            let config = caelix_memory::schema::MemoryVaultConfig::default();
            let vault = MemoryVault::new(config);
            vault
                .init()
                .await
                .map_err(|e| ApiError::InternalError(format!("初始化 MemoryVault 失败: {}", e)))?;
            *guard = Some(Arc::new(vault));
        }
        Ok(guard.as_ref().unwrap().clone())
    }
}

impl Default for MemoryService {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) async fn memory_recall(
    service: &MemoryService,
    query: &str,
    top_k: u32,
) -> Result<Vec<MemoryRecallResult>, ApiError> {
    let vault = service.get_vault().await?;
    let results = vault
        .recall(query, top_k as usize)
        .await
        .map_err(|e| ApiError::InternalError(format!("记忆检索失败: {}", e)))?;

    Ok(results
        .into_iter()
        .map(|r| MemoryRecallResult {
            layer: r.layer,
            file: r.file,
            heading: r.heading,
            preview: r.preview,
            confidence: r.confidence,
        })
        .collect())
}

pub(crate) async fn memory_write(
    service: &MemoryService,
    content: &str,
    source: &str,
    tags: Vec<String>,
) -> Result<(), ApiError> {
    let vault = service.get_vault().await?;

    let src = match source {
        "meeting" => RawSource::Meeting,
        "tweet" => RawSource::Tweet,
        "paper" => RawSource::Paper,
        "note" => RawSource::Note,
        _ => RawSource::Chat,
    };

    let today = chrono::Utc::now().date_naive();
    let heading = chrono::Utc::now().format("%H:%M").to_string();

    vault
        .write_raw(today, src, tags, &heading, content)
        .await
        .map_err(|e| ApiError::InternalError(format!("写入记忆失败: {}", e)))?;

    Ok(())
}

pub(crate) async fn memory_promote_raw(
    _service: &MemoryService,
    file: &str,
) -> Result<(), ApiError> {
    tracing::info!("手动触发 Raw→Wiki 晋升: {}", file);
    Ok(())
}

pub(crate) async fn memory_promote_wiki(
    _service: &MemoryService,
    entity: &str,
) -> Result<(), ApiError> {
    tracing::info!("手动触发 Wiki→Axiom 晋升: {}", entity);
    Ok(())
}

pub(crate) async fn memory_list_conflicts(
    service: &MemoryService,
    all: bool,
) -> Result<Vec<MemoryConflict>, ApiError> {
    let vault = service.get_vault().await?;
    let conflicts = vault
        .list_conflicts(all)
        .await
        .map_err(|e| ApiError::InternalError(format!("获取冲突列表失败: {}", e)))?;

    Ok(conflicts
        .into_iter()
        .map(|c| MemoryConflict {
            id: c.id,
            r#type: c.r#type,
            entity: c.entity,
            field: c.field,
            status: c.status,
            values: c.values,
        })
        .collect())
}

pub(crate) async fn memory_list_candidates(
    service: &MemoryService,
    all: bool,
) -> Result<Vec<MemoryCandidate>, ApiError> {
    let vault = service.get_vault().await?;
    let candidates = vault
        .list_candidates(all)
        .await
        .map_err(|e| ApiError::InternalError(format!("获取候选列表失败: {}", e)))?;

    Ok(candidates
        .into_iter()
        .map(|c| MemoryCandidate {
            id: c.id,
            confidence: c.confidence,
            status: c.status,
            preview: c.preview,
        })
        .collect())
}

pub(crate) async fn memory_rebuild_index(service: &MemoryService) -> Result<(), ApiError> {
    let vault = service.get_vault().await?;
    vault
        .rebuild_index()
        .await
        .map_err(|e| ApiError::InternalError(format!("重建索引失败: {}", e)))?;
    Ok(())
}

pub(crate) async fn memory_stats(service: &MemoryService) -> Result<MemoryStats, ApiError> {
    let vault = service.get_vault().await?;
    let stats = vault
        .stats()
        .await
        .map_err(|e| ApiError::InternalError(format!("获取统计失败: {}", e)))?;

    Ok(MemoryStats {
        raw_files: stats.raw_files,
        wiki_entities: stats.wiki_entities,
        wiki_events: stats.wiki_events,
        axioms: stats.axioms,
        axioms_active: stats.axioms_active,
        pending_conflicts: stats.pending_conflicts,
        pending_candidates: stats.pending_candidates,
        pending_links: stats.pending_links,
        llm_budget_used: stats.llm_budget_used,
        llm_budget_total: stats.llm_budget_total,
    })
}

pub(crate) async fn memory_list_axioms(
    service: &MemoryService,
    include_deprecated: bool,
) -> Result<Vec<MemoryAxiom>, ApiError> {
    let vault = service.get_vault().await?;
    let axioms = vault
        .list_axioms(include_deprecated)
        .await
        .map_err(|e| ApiError::InternalError(format!("获取 Axiom 列表失败: {}", e)))?;

    Ok(axioms
        .into_iter()
        .map(|a| MemoryAxiom {
            name: a.name,
            category: a.category,
            status: a.status,
            confidence: a.confidence,
            created_at: a.created_at.format("%Y-%m-%d %H:%M").to_string(),
            deprecated_reason: a.deprecated_reason,
        })
        .collect())
}

pub(crate) async fn memory_budget(service: &MemoryService) -> Result<MemoryBudgetInfo, ApiError> {
    let vault = service.get_vault().await?;
    let info = vault.get_budget_info().await;

    Ok(MemoryBudgetInfo {
        used: info.used,
        budget: info.budget,
        remaining: info.remaining,
        exhausted: info.exhausted,
    })
}
