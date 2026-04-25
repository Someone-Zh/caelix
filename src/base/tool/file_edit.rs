use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::path::Path;
use tokio::fs;

use crate::base::tool::{Tool, ToolResult};

// 最大可编辑文件大小：10MB
const MAX_EDIT_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DiffEditTool;

#[async_trait]
impl Tool for DiffEditTool {
    fn name(&self) -> &str {
        "diff_edit"
    }

    fn description(&self) -> &str {
        "Apply a unified diff to edit files, create if not exists"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path to create or edit"
                },
                "diff_content": {
                    "type": "string",
                    "description": "Unified diff content (required for editing existing files)"
                },
                "content": {
                    "type": "string",
                    "description": "Direct content to write for new files"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let file_path = match input["file_path"].as_str() {
            Some(v) => v,
            None => return ToolResult {
                output: String::new(),
                error: Some("Missing parameter: file_path".to_string()),
            },
        };

        let diff_content = input["diff_content"].as_str().unwrap_or("");
        let direct_content = input["content"].as_str();

        let path = Path::new(file_path);
        let file_exists = path.exists() && path.is_file();

        let result_content = if !file_exists {
            if let Some(c) = direct_content {
                c.to_string()
            } else if !diff_content.is_empty() {
                match parse_and_apply_unified_diff(&[], diff_content) {
                    Ok(lines) => lines.join("\n"),
                    Err(e) => return ToolResult {
                        output: String::new(),
                        error: Some(format!("Failed to create from diff: {}", e)),
                    },
                }
            } else {
                String::new()
            }
        } else {
            if diff_content.is_empty() {
                return ToolResult {
                    output: String::new(),
                    error: Some("diff_content is required for existing files".to_string()),
                };
            }

            let original = match fs::read_to_string(path).await {
                Ok(c) => c,
                Err(e) => return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read file: {}", e)),
                },
            };

            // 检查文件大小限制
            if original.len() > MAX_EDIT_SIZE {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("File too large: {} bytes (max: {} bytes)", original.len(), MAX_EDIT_SIZE)),
                };
            }

            let lines: Vec<&str> = original.lines().collect();
            match parse_and_apply_unified_diff(&lines, diff_content) {
                Ok(res) => res.join("\n"),
                Err(e) => return ToolResult {
                    output: String::new(),
                    error: Some(format!("Diff apply failed: {}", e)),
                },
            }
        };

        if let Err(e) = fs::write(path, result_content).await {
            return ToolResult {
                output: String::new(),
                error: Some(format!("Failed to write file: {}", e)),
            };
        }

        let msg = if file_exists {
            format!("Successfully edited file: {}", file_path)
        } else {
            format!("Successfully created file: {}", file_path)
        };

        ToolResult {
            output: msg,
            error: None,
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

fn parse_and_apply_unified_diff(
    original: &[&str],
    diff_text: &str,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut ptr = 0;
    let lines: Vec<&str> = diff_text.trim().lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        i += 1;

        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") || line.is_empty() {
            continue;
        }

        let prefix = match line.chars().next() {
            Some(c) => c,
            None => continue,
        };

        let body = &line[1..];

        match prefix {
            ' ' => {
                if ptr >= original.len() {
                    return Err("Context line out of bounds".into());
                }
                if original[ptr] != body {
                    return Err(format!("Context mismatch: expected '{}', got '{}'", body, original[ptr]));
                }
                result.push(body.to_string());
                ptr += 1;
            }

            '-' => {
                if ptr >= original.len() {
                    return Err("Delete line out of bounds".into());
                }
                if original[ptr] != body {
                    return Err(format!("Delete mismatch: expected '{}', got '{}'", body, original[ptr]));
                }
                ptr += 1;
            }

            '+' => {
                result.push(body.to_string());
            }

            _ => continue,
        }
    }

    while ptr < original.len() {
        result.push(original[ptr].to_string());
        ptr += 1;
    }

    Ok(result)
}