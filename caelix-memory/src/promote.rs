use crate::schema::{AxiomCategory, ConflictValue, Layer, WikiEntityCategory};
use crate::vault::{MemoryVault, PromoteTrigger};
use chrono::Utc;
use std::sync::Arc;

pub struct PromoteEngine {
    vault: Arc<MemoryVault>,
}

impl PromoteEngine {
    pub fn new(vault: Arc<MemoryVault>) -> Self {
        Self { vault }
    }

    pub async fn check_triggers(&self) -> Vec<PromoteTrigger> {
        self.vault.check_promote_triggers().await
    }

    pub async fn promote_raw_to_wiki(&self, entity_name: &str) -> anyhow::Result<PromotionResult> {
        let raw_entries = self.vault.get_raw_layer().get_today_entries().await?;

        let mut content = String::new();
        let mut sources = Vec::new();

        for (heading, entry_content) in raw_entries {
            if entry_content.contains(entity_name) {
                content.push_str(&format!("## {}\n{}\n\n", heading, entry_content));
                sources.push(format!(
                    "Raw/{}.md#{}",
                    Utc::now().date_naive().format("%Y-%m-%d"),
                    heading
                ));
            }
        }

        if content.is_empty() {
            return Err(anyhow::anyhow!(
                "No Raw entries found for entity: {}",
                entity_name
            ));
        }

        self.vault
            .write_wiki_entity(
                entity_name,
                WikiEntityCategory::Person,
                Vec::new(),
                Vec::new(),
                0.7,
                sources,
                &content,
            )
            .await?;

        self.write_promotion_log(&format!("Raw → Wiki: {}", entity_name))
            .await?;

        Ok(PromotionResult {
            success: true,
            entity_name: entity_name.to_string(),
            from_layer: Layer::Raw,
            to_layer: Layer::Wiki,
            message: "Created Wiki entity from Raw entries".to_string(),
        })
    }

    pub async fn promote_wiki_to_axiom(
        &self,
        entity_name: &str,
    ) -> anyhow::Result<PromotionResult> {
        let entity = match self.vault.get_wiki_entity_layer().read(entity_name).await? {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!("Wiki entity not found: {}", entity_name));
            }
        };

        let confidence = entity.frontmatter.confidence;
        let config = self.vault.config();

        if confidence < config.promote.wiki_confidence_threshold {
            return Err(anyhow::anyhow!(
                "Confidence {} below threshold {}",
                confidence,
                config.promote.wiki_confidence_threshold
            ));
        }

        if entity.frontmatter.derived_from.len() < config.promote.wiki_derived_from_min as usize {
            return Err(anyhow::anyhow!(
                "Derived from count {} below minimum {}",
                entity.frontmatter.derived_from.len(),
                config.promote.wiki_derived_from_min
            ));
        }

        let conflict_manager = self.vault.get_conflict_manager();
        let existing_axioms = self.vault.get_axiom_layer().list_all().await?;

        let mut conflicts = Vec::new();
        for axiom in &existing_axioms {
            if axiom.body.contains(entity_name) || entity.body.contains(&axiom.name) {
                conflicts.push(axiom.name.clone());
            }
        }

        if !conflicts.is_empty() {
            let values = conflicts
                .iter()
                .map(|name| ConflictValue {
                    value: name.clone(),
                    source: format!("Axioms/{}.md", name),
                    confidence: 0.9,
                })
                .collect();

            conflict_manager
                .write()
                .await
                .add_axiom_conflict(entity_name, values);
            conflict_manager.read().await.save().await?;

            return Ok(PromotionResult {
                success: false,
                entity_name: entity_name.to_string(),
                from_layer: Layer::Wiki,
                to_layer: Layer::Axiom,
                message: format!(
                    "Axiom conflicts detected, added to flags for manual review: {}",
                    conflicts.join(", ")
                ),
            });
        }

        let auto_threshold = config.promote.axiom_auto_promote_confidence;
        let candidate_threshold = config.promote.axiom_candidate_confidence_min;

        if confidence >= auto_threshold {
            let content = format!(
                "## 适用场景\n\n## 例外条件\n\n## 溯源\n本公理由 [[{}]] 实体推导而来。",
                entity_name
            );

            let category = AxiomCategory::Rule;
            let axiom_name = format!("{}_rule", entity_name);

            self.vault
                .write_axiom(
                    &axiom_name,
                    category,
                    confidence,
                    entity.frontmatter.derived_from,
                    &content,
                )
                .await?;

            self.write_promotion_log(&format!(
                "Wiki → Axiom: {} (confidence: {:.2})",
                entity_name, confidence
            ))
            .await?;

            Ok(PromotionResult {
                success: true,
                entity_name: entity_name.to_string(),
                from_layer: Layer::Wiki,
                to_layer: Layer::Axiom,
                message: format!("Created Axiom from Wiki entity: {}", entity_name),
            })
        } else if confidence >= candidate_threshold {
            let draft = format!(
                "# {}\n\n## 适用场景\n\n## 例外条件\n\n## 溯源\n本公理由 [[{}]] 实体推导而来。\n\nConfidence: {:.2}",
                format!("{}_rule", entity_name),
                entity_name,
                confidence
            );

            conflict_manager.write().await.add_axiom_candidate(
                &draft,
                entity.frontmatter.derived_from.clone(),
                confidence,
            );
            conflict_manager.read().await.save().await?;

            self.write_promotion_log(&format!(
                "Wiki → Axiom candidate: {} (confidence: {:.2}, waiting for approval)",
                entity_name, confidence
            ))
            .await?;

            Ok(PromotionResult {
                success: true,
                entity_name: entity_name.to_string(),
                from_layer: Layer::Wiki,
                to_layer: Layer::Axiom,
                message: format!(
                    "Axiom candidate created, waiting for approval: {}",
                    entity_name
                ),
            })
        } else {
            self.write_promotion_log(&format!(
                "Wiki → Axiom rejected: {} (confidence: {:.2} below threshold)",
                entity_name, confidence
            ))
            .await?;

            Ok(PromotionResult {
                success: false,
                entity_name: entity_name.to_string(),
                from_layer: Layer::Wiki,
                to_layer: Layer::Axiom,
                message: format!(
                    "Confidence {:.2} below candidate threshold {}",
                    confidence, candidate_threshold
                ),
            })
        }
    }

    async fn write_promotion_log(&self, message: &str) -> anyhow::Result<()> {
        let log_dir = self.vault.config().root_dir.clone() + "/Meta";
        let log_path = format!("{}/promotion_log.md", log_dir);

        tokio::fs::create_dir_all(&log_dir).await?;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M");
        let log_entry = format!("\n## {} PromoteWorker\n\n* {}\n", timestamp, message);

        tokio::fs::write(&log_path, log_entry).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PromotionResult {
    pub success: bool,
    pub entity_name: String,
    pub from_layer: Layer,
    pub to_layer: Layer,
    pub message: String,
}

impl PromotionResult {
    pub fn to_string(&self) -> String {
        format!(
            "Promotion {}: {} from {:?} to {:?}\n{}",
            if self.success { "successful" } else { "failed" },
            self.entity_name,
            self.from_layer,
            self.to_layer,
            self.message
        )
    }
}
