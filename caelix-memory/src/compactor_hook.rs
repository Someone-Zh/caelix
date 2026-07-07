use caelix_api::hooks::{AgentHook, HookCapability, HookScope, PostToolExecContext, PreToolExecContext};
use caelix_api::task::{Runnable, TaskKind};
use caelix_task::TaskManager;
use crate::vault::{MemoryVault, PromoteTrigger};
use serde_json;
use std::sync::Arc;

pub struct MemoryCompactorHook {
    vault: Arc<MemoryVault>,
    task_manager: Arc<TaskManager>,
}

impl MemoryCompactorHook {
    pub fn new(vault: Arc<MemoryVault>, task_manager: Arc<TaskManager>) -> Self {
        Self { vault, task_manager }
    }
}

#[async_trait::async_trait]
impl AgentHook for MemoryCompactorHook {
    fn name(&self) -> &str {
        "memory_compactor_hook"
    }

    fn capabilities(&self) -> HookCapability {
        HookCapability::POST_TOOL_EXEC
    }

    async fn on_post_tool_exec(&self, ctx: &mut PostToolExecContext) -> Result<(), anyhow::Error> {
        if ctx.tool_name == "memory_write" {
            self.check_and_trigger_promotions().await;
        }
        Ok(())
    }
}

impl MemoryCompactorHook {
    async fn check_and_trigger_promotions(&self) {
        let triggers = self.vault.check_promote_triggers().await;

        for trigger in triggers {
            match trigger {
                PromoteTrigger::RawParagraphThreshold(count) => {
                    tracing::info!(
                        "MemoryCompactorHook: Raw paragraphs exceeded threshold ({}/{}), triggering promotion",
                        count,
                        self.vault.config().promote.raw_paragraphs_per_day
                    );
                    self.submit_promotion_tasks().await;
                }
                PromoteTrigger::WikiEntityReady(entity_name, confidence) => {
                    tracing::info!(
                        "MemoryCompactorHook: Wiki entity '{}' ready for promotion (confidence: {:.2})",
                        entity_name, confidence
                    );
                    self.submit_wiki_to_axiom_task(&entity_name, confidence).await;
                }
                PromoteTrigger::NewEntity(entity_name) => {
                    tracing::info!(
                        "MemoryCompactorHook: New entity '{}' detected",
                        entity_name
                    );
                    self.submit_raw_to_wiki_task(&entity_name).await;
                }
            }
        }
    }

    async fn submit_promotion_tasks(&self) {
        let today_entries = match self.vault.get_raw_layer().get_today_entries().await {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut entity_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_heading, content) in today_entries {
            let links = crate::link::Link::extract_entity_names(&content);
            for name in links {
                *entity_counts.entry(name).or_insert(0) += 1;
            }
        }

        let threshold = self.vault.config().promote.raw_mentions_per_entity;
        for (entity_name, count) in entity_counts {
            if count >= threshold as usize {
                self.submit_raw_to_wiki_task(&entity_name).await;
            }
        }
    }

    async fn submit_raw_to_wiki_task(&self, entity_name: &str) {
        let runnable = RawToWikiPromoteRunnable {
            vault: self.vault.clone(),
            entity_name: entity_name.to_string(),
        };

        let _ = self
            .task_manager
            .submit(
                None,
                Some(format!("MemoryCompactor: Raw → Wiki: {}", entity_name)),
                TaskKind::Async,
                Box::new(runnable),
            )
            .await;
    }

    async fn submit_wiki_to_axiom_task(&self, entity_name: &str, confidence: f64) {
        let auto_threshold = self.vault.config().promote.axiom_auto_promote_confidence;
        if confidence < auto_threshold {
            return;
        }

        let runnable = WikiToAxiomPromoteRunnable {
            vault: self.vault.clone(),
            entity_name: entity_name.to_string(),
        };

        let _ = self
            .task_manager
            .submit(
                None,
                Some(format!("MemoryCompactor: Wiki → Axiom: {}", entity_name)),
                TaskKind::Async,
                Box::new(runnable),
            )
            .await;
    }
}

struct RawToWikiPromoteRunnable {
    vault: Arc<MemoryVault>,
    entity_name: String,
}

#[async_trait::async_trait]
impl Runnable for RawToWikiPromoteRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        let engine = crate::promote::PromoteEngine::new(self.vault.clone());
        match engine.promote_raw_to_wiki(&self.entity_name).await {
            Ok(r) => Ok(r.to_string()),
            Err(e) => Err(caelix_api::error::AgentError::TaskError(format!(
                "Raw → Wiki promotion failed: {}",
                e
            ))),
        }
    }

    fn task_type(&self) -> &'static str {
        "memory_compactor_raw_to_wiki"
    }

    fn payload(&self) -> String {
        serde_json::to_string(&serde_json::json!({
            "entity_name": self.entity_name,
            "type": "raw_to_wiki"
        })).unwrap_or_default()
    }
}

struct WikiToAxiomPromoteRunnable {
    vault: Arc<MemoryVault>,
    entity_name: String,
}

#[async_trait::async_trait]
impl Runnable for WikiToAxiomPromoteRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        let engine = crate::promote::PromoteEngine::new(self.vault.clone());
        match engine.promote_wiki_to_axiom(&self.entity_name).await {
            Ok(r) => Ok(r.to_string()),
            Err(e) => Err(caelix_api::error::AgentError::TaskError(format!(
                "Wiki → Axiom promotion failed: {}",
                e
            ))),
        }
    }

    fn task_type(&self) -> &'static str {
        "memory_compactor_wiki_to_axiom"
    }

    fn payload(&self) -> String {
        serde_json::to_string(&serde_json::json!({
            "entity_name": self.entity_name,
            "type": "wiki_to_axiom"
        })).unwrap_or_default()
    }
}