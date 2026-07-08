use crate::schema::RawSource;
use crate::vault::MemoryVault;
use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolResult};
use chrono::Utc;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryWriteTool {
    vault: Arc<MemoryVault>,
}

impl MemoryWriteTool {
    pub fn new(vault: Arc<MemoryVault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl Tool for MemoryWriteTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn description(&self) -> &str {
        "Write memory to the Memory Vault system. Default writes to Raw layer. Can also write to Wiki layer (entities/events). Axiom layer can only be written via promotion."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to write to memory"
                },
                "layer": {
                    "type": "string",
                    "description": "Target layer: 'raw' (default), 'wiki_entity', 'wiki_event'",
                    "enum": ["raw", "wiki_entity", "wiki_event"],
                    "default": "raw"
                },
                "heading": {
                    "type": "string",
                    "description": "Heading/title for the entry (for Raw and Wiki layers)"
                },
                "source": {
                    "type": "string",
                    "description": "Source of the memory: 'chat', 'meeting', 'tweet', 'paper', 'note'",
                    "enum": ["chat", "meeting", "tweet", "paper", "note"],
                    "default": "chat"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for categorization"
                },
                "entity_name": {
                    "type": "string",
                    "description": "Entity name (required for wiki_entity layer)"
                },
                "entity_category": {
                    "type": "string",
                    "description": "Entity category: 'person', 'project', 'technology', 'organization'",
                    "enum": ["person", "project", "technology", "organization"]
                },
                "aliases": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Alternative names for the entity"
                },
                "event_name": {
                    "type": "string",
                    "description": "Event name (required for wiki_event layer)"
                },
                "participants": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Participants in the event"
                },
                "related_entities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Entities related to the event"
                },
                "confidence": {
                    "type": "number",
                    "description": "Confidence level (0.0-1.0) for Wiki entities/events",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "default": 0.7
                },
                "derived_from": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Source references (for Wiki layer)"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let layer = input["layer"].as_str().unwrap_or("raw");

        match layer {
            "raw" => self.execute_write_raw(input).await,
            "wiki_entity" => self.execute_write_wiki_entity(input).await,
            "wiki_event" => self.execute_write_wiki_event(input).await,
            "axiom" => ToolResult {
                output: String::new(),
                error: Some(
                    "Axiom layer cannot be written directly. Use memory_promote tool instead."
                        .to_string(),
                ),
            },
            _ => ToolResult {
                output: String::new(),
                error: Some(format!("Unknown layer: {}", layer)),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

impl MemoryWriteTool {
    async fn execute_write_raw(&self, input: JsonValue) -> ToolResult {
        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: content".to_string()),
                };
            }
        };

        let heading = input["heading"].as_str().unwrap_or("");
        let source_str = input["source"].as_str().unwrap_or("chat");
        let tags: Vec<String> = input["tags"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let source = match source_str {
            "chat" => RawSource::Chat,
            "meeting" => RawSource::Meeting,
            "tweet" => RawSource::Tweet,
            "paper" => RawSource::Paper,
            "note" => RawSource::Note,
            _ => RawSource::Chat,
        };

        let date = Utc::now().date_naive();
        let heading_text = if heading.is_empty() {
            format!("{} {}", date.format("%H:%M"), "记忆条目")
        } else {
            heading.to_string()
        };

        match self
            .vault
            .write_raw(date, source, tags, &heading_text, content)
            .await
        {
            Ok(_) => ToolResult {
                output: format!(
                    "Successfully wrote to Raw layer\nDate: {}\nHeading: {}\nContent preview: {}",
                    date.format("%Y-%m-%d"),
                    heading_text,
                    truncate_content(content)
                ),
                error: None,
            },
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(format!("Failed to write to Raw layer: {}", e)),
            },
        }
    }

    async fn execute_write_wiki_entity(&self, input: JsonValue) -> ToolResult {
        let name = match input["entity_name"].as_str() {
            Some(n) => n,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: entity_name".to_string()),
                };
            }
        };

        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: content".to_string()),
                };
            }
        };

        let category_str = input["entity_category"].as_str().unwrap_or("person");
        let category = match category_str {
            "person" => crate::schema::WikiEntityCategory::Person,
            "project" => crate::schema::WikiEntityCategory::Project,
            "technology" => crate::schema::WikiEntityCategory::Technology,
            "organization" => crate::schema::WikiEntityCategory::Organization,
            _ => crate::schema::WikiEntityCategory::Person,
        };

        let aliases: Vec<String> = input["aliases"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let tags: Vec<String> = input["tags"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let confidence = input["confidence"].as_f64().unwrap_or(0.7);
        let derived_from: Vec<String> = input["derived_from"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        match self.vault.write_wiki_entity(name, category, aliases, tags, confidence, derived_from, content).await {
            Ok(_) => ToolResult {
                output: format!("Successfully wrote Wiki Entity: {}\nCategory: {}\nConfidence: {:.2}\nContent preview: {}", name, category_str, confidence, truncate_content(content)),
                error: None,
            },
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(format!("Failed to write Wiki Entity: {}", e)),
            },
        }
    }

    async fn execute_write_wiki_event(&self, input: JsonValue) -> ToolResult {
        let name = match input["event_name"].as_str() {
            Some(n) => n,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: event_name".to_string()),
                };
            }
        };

        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: content".to_string()),
                };
            }
        };

        let participants: Vec<String> = input["participants"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let related_entities: Vec<String> = input["related_entities"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let confidence = input["confidence"].as_f64().unwrap_or(0.7);
        let derived_from: Vec<String> = input["derived_from"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();

        let today = Utc::now().date_naive();
        let date_range = vec![today, today];

        match self
            .vault
            .write_wiki_event(
                name,
                date_range,
                participants,
                related_entities,
                confidence,
                derived_from,
                content,
            )
            .await
        {
            Ok(_) => ToolResult {
                output: format!(
                    "Successfully wrote Wiki Event: {}\nConfidence: {:.2}\nContent preview: {}",
                    name,
                    confidence,
                    truncate_content(content)
                ),
                error: None,
            },
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(format!("Failed to write Wiki Event: {}", e)),
            },
        }
    }
}

fn truncate_content(content: &str) -> String {
    if content.len() > 100 {
        format!("{}...", &content[..100])
    } else {
        content.to_string()
    }
}
