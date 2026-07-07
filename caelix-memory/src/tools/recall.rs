use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolResult};
use crate::vault::{MemoryVault, RecallResult};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryRecallTool {
    vault: Arc<MemoryVault>,
}

impl MemoryRecallTool {
    pub fn new(vault: Arc<MemoryVault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    fn name(&self) -> &str {
        "memory_recall"
    }

    fn description(&self) -> &str {
        "Recall memories from the Memory Vault. Supports weighted search across Raw, Wiki, and Axiom layers. Returns results with layer labels for credibility context."
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query - entity name or keyword to recall"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "minimum": 1,
                    "maximum": 50,
                    "default": 5
                },
                "include_raw": {
                    "type": "boolean",
                    "description": "Include results from Raw layer (default: true)",
                    "default": true
                },
                "include_wiki": {
                    "type": "boolean",
                    "description": "Include results from Wiki layer (default: true)",
                    "default": true
                },
                "include_axiom": {
                    "type": "boolean",
                    "description": "Include results from Axiom layer (default: true)",
                    "default": true
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let query = match input["query"].as_str() {
            Some(q) => q,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: query".to_string()),
                };
            }
        };

        let top_k = input["top_k"].as_u64().unwrap_or(5) as usize;

        match self.vault.recall(query, top_k).await {
            Ok(results) => {
                let output = format_results(&results);
                ToolResult {
                    output,
                    error: None,
                }
            }
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(format!("Failed to recall memory: {}", e)),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

fn format_results(results: &[RecallResult]) -> String {
    if results.is_empty() {
        return "No memory results found.".to_string();
    }

    let mut output = format!("Memory Recall Results ({} items):\n\n", results.len());

    for (i, result) in results.iter().enumerate() {
        let confidence_str = match result.confidence {
            Some(c) => format!(" (confidence: {:.2})", c),
            None => String::new(),
        };

        let layer_label = match result.layer.as_str() {
            "Axiom" => "[AXIOM]",
            "Wiki" => "[WIKI]",
            "Raw" => "[RAW]",
            _ => "[UNKNOWN]",
        };

        output.push_str(&format!(
            "{}. {} {} {}\n",
            i + 1,
            layer_label,
            result.heading,
            confidence_str
        ));
        output.push_str(&format!("   File: {}\n", result.file));
        output.push_str(&format!("   Preview: {}\n\n", result.preview));
    }

    output.push_str("\nNote: Results are weighted by layer (Axiom: 1.0, Wiki: 0.7, Raw: 0.3) and sorted by recency within layer.\n");

    output
}