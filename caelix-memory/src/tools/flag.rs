use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolResult};
use crate::vault::MemoryVault;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryFlagTool {
    vault: Arc<MemoryVault>,
}

impl MemoryFlagTool {
    pub fn new(vault: Arc<MemoryVault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl Tool for MemoryFlagTool {
    fn name(&self) -> &str {
        "memory_flag"
    }

    fn description(&self) -> &str {
        "View and manage conflicts, pending links, and axiom candidates in the Memory Vault."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: 'list', 'resolve', 'approve', 'reject'",
                    "enum": ["list", "resolve", "approve", "reject"],
                    "default": "list"
                },
                "id": {
                    "type": "string",
                    "description": "ID of the conflict or candidate to resolve/approve/reject"
                },
                "filter": {
                    "type": "string",
                    "description": "Filter type: 'all', 'conflicts', 'candidates', 'pending_links'",
                    "enum": ["all", "conflicts", "candidates", "pending_links"],
                    "default": "all"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let action = input["action"].as_str().unwrap_or("list");

        match action {
            "list" => self.execute_list(input).await,
            "resolve" => self.execute_resolve(input).await,
            "approve" => self.execute_approve(input).await,
            "reject" => self.execute_reject(input).await,
            _ => ToolResult {
                output: String::new(),
                error: Some(format!("Unknown action: {}", action)),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

impl MemoryFlagTool {
    async fn execute_list(&self, input: JsonValue) -> ToolResult {
        let filter = input["filter"].as_str().unwrap_or("all");
        let conflict_mgr = self.vault.get_conflict_manager();
        let conflict = conflict_mgr.read().await;

        let conflicts: Vec<crate::schema::Contradiction> = match filter {
            "all" | "conflicts" => conflict.get_pending_conflicts().into_iter().cloned().collect(),
            _ => Vec::new(),
        };

        let candidates: Vec<crate::schema::AxiomCandidate> = match filter {
            "all" | "candidates" => conflict.get_pending_candidates().into_iter().cloned().collect(),
            _ => Vec::new(),
        };

        let links: Vec<crate::schema::PendingLink> = match filter {
            "all" | "pending_links" => conflict.get_pending_links().into_iter().cloned().collect(),
            _ => Vec::new(),
        };

        drop(conflict);

        let mut output = String::new();

        if !conflicts.is_empty() {
            output.push_str(&format!("Pending Conflicts ({}):\n", conflicts.len()));
            for c in conflicts {
                output.push_str(&format!("  ID: {}\n", c.id));
                output.push_str(&format!("  Type: {:?}\n", c.r#type));
                output.push_str(&format!("  Entity: {}\n", c.entity));
                if let Some(field) = &c.field {
                    output.push_str(&format!("  Field: {}\n", field));
                }
                output.push_str("  Values:\n");
                for v in &c.values {
                    output.push_str(&format!("    - {} (source: {}, confidence: {:.2})\n", v.value, v.source, v.confidence));
                }
                output.push('\n');
            }
        }

        if !candidates.is_empty() {
            output.push_str(&format!("Pending Axiom Candidates ({}):\n", candidates.len()));
            for c in candidates {
                output.push_str(&format!("  ID: {}\n", c.id));
                output.push_str(&format!("  Confidence: {:.2}\n", c.confidence));
                output.push_str(&format!("  Derived from: {}\n", c.derived_from.join(", ")));
                output.push_str(&format!("  Draft preview: {}\n\n", truncate(&c.draft, 100)));
            }
        }

        if !links.is_empty() {
            output.push_str(&format!("Pending Links ({}):\n", links.len()));
            for l in links {
                output.push_str(&format!("  From: {}\n", l.from));
                output.push_str(&format!("  Link: {}\n\n", l.link));
            }
        }

        if output.is_empty() {
            output = "No pending items found.".to_string();
        }

        ToolResult {
            output,
            error: None,
        }
    }

    async fn execute_resolve(&self, input: JsonValue) -> ToolResult {
        let id = match input["id"].as_str() {
            Some(i) => i.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: id".to_string()),
                };
            }
        };

        let conflict_mgr = self.vault.get_conflict_manager();
        let mut conflict = conflict_mgr.write().await;
        let success = conflict.resolve_conflict(&id, Vec::new());

        if success {
            conflict.save().await.ok();
            ToolResult {
                output: format!("Successfully resolved conflict: {}", id),
                error: None,
            }
        } else {
            ToolResult {
                output: String::new(),
                error: Some(format!("Conflict not found: {}", id)),
            }
        }
    }

    async fn execute_approve(&self, input: JsonValue) -> ToolResult {
        let id = match input["id"].as_str() {
            Some(i) => i.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: id".to_string()),
                };
            }
        };

        let conflict_mgr = self.vault.get_conflict_manager();
        let mut conflict = conflict_mgr.write().await;
        let success = conflict.approve_candidate(&id);

        if success {
            conflict.save().await.ok();
            ToolResult {
                output: format!("Successfully approved axiom candidate: {}", id),
                error: None,
            }
        } else {
            ToolResult {
                output: String::new(),
                error: Some(format!("Candidate not found: {}", id)),
            }
        }
    }

    async fn execute_reject(&self, input: JsonValue) -> ToolResult {
        let id = match input["id"].as_str() {
            Some(i) => i.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: id".to_string()),
                };
            }
        };

        let conflict_mgr = self.vault.get_conflict_manager();
        let mut conflict = conflict_mgr.write().await;
        let success = conflict.reject_candidate(&id);

        if success {
            conflict.save().await.ok();
            ToolResult {
                output: format!("Successfully rejected axiom candidate: {}", id),
                error: None,
            }
        } else {
            ToolResult {
                output: String::new(),
                error: Some(format!("Candidate not found: {}", id)),
            }
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}