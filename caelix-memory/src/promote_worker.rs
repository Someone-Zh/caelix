use crate::budget::LlmBudgetManager;
use crate::promote::PromoteEngine;
use crate::schema::Layer;
use crate::vault::MemoryVault;
use caelix_api::task::{Runnable, TaskKind};
use caelix_task::TaskManager;
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::{self, json};
use std::sync::Arc;

pub struct PromoteWorker {
    vault: Arc<MemoryVault>,
    task_manager: Arc<TaskManager>,
}

impl PromoteWorker {
    pub fn new(vault: Arc<MemoryVault>, task_manager: Arc<TaskManager>) -> Self {
        Self {
            vault,
            task_manager,
        }
    }

    pub async fn run(&self) {
        loop {
            self.process_pending_tasks().await;
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    }

    async fn process_pending_tasks(&self) {
        let triggers = self.vault.check_promote_triggers().await;

        for trigger in triggers {
            match trigger {
                crate::vault::PromoteTrigger::RawParagraphThreshold(_) => {
                    self.process_raw_to_wiki_promotion().await;
                }
                crate::vault::PromoteTrigger::WikiEntityReady(entity_name, confidence) => {
                    if confidence >= self.vault.config().promote.wiki_confidence_threshold {
                        self.submit_wiki_to_axiom_task(&entity_name).await;
                    }
                }
                crate::vault::PromoteTrigger::NewEntity(entity_name) => {
                    self.submit_raw_to_wiki_task(&entity_name).await;
                }
            }
        }
    }

    async fn process_raw_to_wiki_promotion(&self) {
        let today_entries = match self.vault.get_raw_layer().get_today_entries().await {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut entity_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
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
        let runnable = RawToWikiRunnable {
            vault: self.vault.clone(),
            entity_name: entity_name.to_string(),
        };

        let _ = self
            .task_manager
            .submit(
                None,
                Some(format!("Raw → Wiki: {}", entity_name)),
                TaskKind::Async,
                Box::new(runnable),
            )
            .await;
    }

    async fn submit_wiki_to_axiom_task(&self, entity_name: &str) {
        let runnable = WikiToAxiomRunnable {
            vault: self.vault.clone(),
            entity_name: entity_name.to_string(),
        };

        let _ = self
            .task_manager
            .submit(
                None,
                Some(format!("Wiki → Axiom: {}", entity_name)),
                TaskKind::Async,
                Box::new(runnable),
            )
            .await;
    }
}

struct RawToWikiRunnable {
    vault: Arc<MemoryVault>,
    entity_name: String,
}

#[async_trait::async_trait]
impl Runnable for RawToWikiRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        let task_id = format!("promote-raw-to-wiki-{}", uuid::Uuid::new_v4());

        let budget = self.vault.get_budget_manager();
        if !budget.write().await.try_acquire(&task_id) {
            return Ok(format!(
                "LLM budget exhausted. Task deferred: {} → Wiki",
                self.entity_name
            ));
        }

        let engine = PromoteEngine::new(self.vault.clone());
        let result = engine.promote_raw_to_wiki(&self.entity_name).await;

        budget.read().await.save().await.ok();

        match result {
            Ok(r) => Ok(r.to_string()),
            Err(e) => Err(caelix_api::error::AgentError::TaskError(format!(
                "Raw → Wiki promotion failed: {}",
                e
            ))),
        }
    }

    fn task_type(&self) -> &'static str {
        "memory_promote_raw_to_wiki"
    }

    fn payload(&self) -> String {
        serde_json::to_string(&json!({
            "entity_name": self.entity_name,
            "type": "raw_to_wiki"
        }))
        .unwrap_or_default()
    }
}

struct WikiToAxiomRunnable {
    vault: Arc<MemoryVault>,
    entity_name: String,
}

#[async_trait::async_trait]
impl Runnable for WikiToAxiomRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        let task_id = format!("promote-wiki-to-axiom-{}", uuid::Uuid::new_v4());

        let budget = self.vault.get_budget_manager();
        if !budget.write().await.try_acquire(&task_id) {
            return Ok(format!(
                "LLM budget exhausted. Task deferred: {} → Axiom",
                self.entity_name
            ));
        }

        let engine = PromoteEngine::new(self.vault.clone());
        let result = engine.promote_wiki_to_axiom(&self.entity_name).await;

        budget.read().await.save().await.ok();

        match result {
            Ok(r) => Ok(r.to_string()),
            Err(e) => Err(caelix_api::error::AgentError::TaskError(format!(
                "Wiki → Axiom promotion failed: {}",
                e
            ))),
        }
    }

    fn task_type(&self) -> &'static str {
        "memory_promote_wiki_to_axiom"
    }

    fn payload(&self) -> String {
        serde_json::to_string(&json!({
            "entity_name": self.entity_name,
            "type": "wiki_to_axiom"
        }))
        .unwrap_or_default()
    }
}

use uuid;
