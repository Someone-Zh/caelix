use crate::vault::MemoryVault;
use async_trait::async_trait;
use caelix_api::task::{Runnable, TaskKind};
use caelix_api::tool::{Tool, ToolResult};
use caelix_task::TaskManager;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryPromoteTool {
    vault: Arc<MemoryVault>,
    task_manager: Arc<TaskManager>,
}

impl MemoryPromoteTool {
    pub fn new(vault: Arc<MemoryVault>, task_manager: Arc<TaskManager>) -> Self {
        Self {
            vault,
            task_manager,
        }
    }
}

#[async_trait]
impl Tool for MemoryPromoteTool {
    fn name(&self) -> &str {
        "memory_promote"
    }

    fn description(&self) -> &str {
        "Manually trigger memory promotion. Can promote from Raw to Wiki or Wiki to Axiom. Returns a task ID for tracking."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["entity_name"],
            "properties": {
                "entity_name": {
                    "type": "string",
                    "description": "Name of the entity to promote"
                },
                "promote_to": {
                    "type": "string",
                    "description": "Target layer: 'wiki' or 'axiom'",
                    "enum": ["wiki", "axiom"],
                    "default": "wiki"
                },
                "confidence": {
                    "type": "number",
                    "description": "Confidence level for promotion (0.0-1.0)",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "default": 0.85
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let entity_name = match input["entity_name"].as_str() {
            Some(n) => n,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: entity_name".to_string()),
                };
            }
        };

        let promote_to = input["promote_to"].as_str().unwrap_or("wiki");
        let confidence = input["confidence"].as_f64().unwrap_or(0.85);

        let _payload = json!({
            "entity_name": entity_name,
            "promote_to": promote_to,
            "confidence": confidence
        });

        let runnable = PromoteRunnable {
            vault: self.vault.clone(),
            entity_name: entity_name.to_string(),
            promote_to: promote_to.to_string(),
            confidence,
        };

        let task_id = self
            .task_manager
            .submit(
                None,
                Some("Memory Promotion".to_string()),
                TaskKind::Async,
                Box::new(runnable),
            )
            .await;

        ToolResult {
            output: format!("Promotion task submitted\nTask ID: {}\nEntity: {}\nPromote to: {}\nConfidence: {:.2}", task_id, entity_name, promote_to, confidence),
            error: None,
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

struct PromoteRunnable {
    vault: Arc<MemoryVault>,
    entity_name: String,
    promote_to: String,
    confidence: f64,
}

#[async_trait]
impl Runnable for PromoteRunnable {
    async fn run(&self) -> Result<String, caelix_api::error::AgentError> {
        let task_id = format!("promote-{}", uuid::Uuid::new_v4());

        let budget = self.vault.get_budget_manager();
        if !budget.write().await.try_acquire(&task_id) {
            return Ok(format!(
                "LLM budget exhausted. Task deferred: {}",
                self.entity_name
            ));
        }

        let result = match self.promote_to.as_str() {
            "wiki" => self.promote_raw_to_wiki().await,
            "axiom" => self.promote_wiki_to_axiom().await,
            _ => Err(anyhow::anyhow!(
                "Unknown promotion target: {}",
                self.promote_to
            )),
        };

        budget.read().await.save().await.ok();

        match result {
            Ok(message) => Ok(format!(
                "Promotion successful: {}\n{}",
                self.entity_name, message
            )),
            Err(e) => Err(caelix_api::error::AgentError::TaskError(format!(
                "Promotion failed: {}",
                e
            ))),
        }
    }

    fn task_type(&self) -> &'static str {
        "memory_promote"
    }

    fn payload(&self) -> String {
        serde_json::to_string(&json!({
            "entity_name": self.entity_name,
            "promote_to": self.promote_to,
            "confidence": self.confidence
        }))
        .unwrap_or_default()
    }
}

impl PromoteRunnable {
    async fn promote_raw_to_wiki(&self) -> anyhow::Result<String> {
        let raw_entries = self.vault.get_raw_layer().get_today_entries().await?;

        let mut content = String::new();
        let mut sources = Vec::new();

        for (heading, entry_content) in raw_entries {
            if entry_content.contains(&self.entity_name) {
                content.push_str(&format!("## {}\n{}\n\n", heading, entry_content));
                sources.push(format!(
                    "Raw/{}.md#{}",
                    chrono::Utc::now().date_naive().format("%Y-%m-%d"),
                    heading
                ));
            }
        }

        if content.is_empty() {
            return Err(anyhow::anyhow!(
                "No Raw entries found for entity: {}",
                self.entity_name
            ));
        }

        self.vault
            .write_wiki_entity(
                &self.entity_name,
                crate::schema::WikiEntityCategory::Person,
                Vec::new(),
                Vec::new(),
                self.confidence,
                sources,
                &content,
            )
            .await?;

        Ok("Created Wiki entity from Raw entries".to_string())
    }

    async fn promote_wiki_to_axiom(&self) -> anyhow::Result<String> {
        let entity = match self
            .vault
            .get_wiki_entity_layer()
            .read(&self.entity_name)
            .await?
        {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!(
                    "Wiki entity not found: {}",
                    self.entity_name
                ))
            }
        };

        let content = format!(
            "## 适用场景\n\n## 例外条件\n\n## 溯源\n本公理由 [[{}]] 实体推导而来。",
            self.entity_name
        );

        let category = crate::schema::AxiomCategory::Rule;

        self.vault
            .write_axiom(
                &format!("{}_rule", self.entity_name),
                category,
                self.confidence,
                entity.frontmatter.derived_from,
                &content,
            )
            .await?;

        Ok(format!(
            "Created Axiom from Wiki entity: {}",
            self.entity_name
        ))
    }
}

use uuid;
