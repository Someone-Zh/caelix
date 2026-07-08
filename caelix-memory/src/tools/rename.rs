use crate::vault::MemoryVault;
use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolResult};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryRenameTool {
    vault: Arc<MemoryVault>,
}

impl MemoryRenameTool {
    pub fn new(vault: Arc<MemoryVault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl Tool for MemoryRenameTool {
    fn name(&self) -> &str {
        "memory_rename"
    }

    fn description(&self) -> &str {
        "Rename an entity or event in the Wiki layer. Updates all references and links across the vault."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["old_name", "new_name"],
            "properties": {
                "old_name": {
                    "type": "string",
                    "description": "Current name of the entity or event"
                },
                "new_name": {
                    "type": "string",
                    "description": "New name for the entity or event"
                },
                "type": {
                    "type": "string",
                    "description": "Type: 'entity' (default) or 'event'",
                    "enum": ["entity", "event"],
                    "default": "entity"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let old_name = match input["old_name"].as_str() {
            Some(n) => n,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: old_name".to_string()),
                };
            }
        };

        let new_name = match input["new_name"].as_str() {
            Some(n) => n,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: new_name".to_string()),
                };
            }
        };

        let entity_type = input["type"].as_str().unwrap_or("entity");

        let result = match entity_type {
            "entity" => self.vault.rename_entity(old_name, new_name).await,
            "event" => self.vault.rename_event(old_name, new_name).await,
            _ => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Unknown type: {}", entity_type)),
                }
            }
        };

        match result {
            Ok(_) => ToolResult {
                output: format!(
                    "Successfully renamed {} '{}' to '{}'",
                    entity_type, old_name, new_name
                ),
                error: None,
            },
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(format!("Failed to rename {}: {}", entity_type, e)),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
