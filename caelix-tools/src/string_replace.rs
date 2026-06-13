use async_trait::async_trait;
use serde_json::{Value as JsonValue, json};
use std::path::Path;
use tokio::fs;

use caelix_api::tool::{Tool, ToolApprovalType, ToolPreCheckResult, ToolResult};

// 最大可编辑文件大小：10MB
const MAX_REPLACE_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct StringReplaceTool;

#[async_trait]
impl Tool for StringReplaceTool {
    fn name(&self) -> &str {
        "string_replace"
    }

    fn description(&self) -> &str {
        "对文件执行多次字符串或正则表达式替换操作，支持批量替换"
    }

    fn pre_check(&self, input: &JsonValue) -> Option<ToolPreCheckResult> {
        let file_path = input["file_path"].as_str()?.to_string();
        Some(ToolPreCheckResult {
            approval_type: ToolApprovalType::Path,
            parameters: json!({ "file_path": file_path }),
        })
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["file_path", "replacements"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "目标文件路径"
                },
                "replacements": {
                    "type": "array",
                    "description": "替换操作列表，按顺序执行",
                    "items": {
                        "type": "object",
                        "required": ["old_text", "new_text"],
                        "properties": {
                            "old_text": {
                                "type": "string",
                                "description": "要查找的文本或正则表达式"
                            },
                            "new_text": {
                                "type": "string",
                                "description": "替换后的文本"
                            },
                            "use_regex": {
                                "type": "boolean",
                                "description": "是否使用正则表达式，默认false",
                                "default": false
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 解析文件路径
        let file_path = match input["file_path"].as_str() {
            Some(v) => v,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: file_path".to_string()),
                };
            }
        };

        // 解析替换操作列表
        let replacements = match input["replacements"].as_array() {
            Some(arr) if !arr.is_empty() => arr,
            _ => {
                return ToolResult {
                    output: String::new(),
                    error: Some(
                        "Missing or empty parameter: replacements (must be a non-empty array)"
                            .to_string(),
                    ),
                };
            }
        };

        let path = Path::new(file_path);

        // 检查文件是否存在
        if !path.exists() || !path.is_file() {
            return ToolResult {
                output: String::new(),
                error: Some(format!(
                    "File does not exist or is not a file: {}",
                    file_path
                )),
            };
        }

        // 读取文件内容
        let content = match fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read file: {}", e)),
                };
            }
        };

        // 检查文件大小限制
        if content.len() > MAX_REPLACE_SIZE {
            return ToolResult {
                output: String::new(),
                error: Some(format!(
                    "File too large: {} bytes (max: {} bytes)",
                    content.len(),
                    MAX_REPLACE_SIZE
                )),
            };
        }

        // 执行替换操作
        let mut current_content = content;
        let mut replacement_stats = Vec::new();

        for (idx, replacement) in replacements.iter().enumerate() {
            let old_text = match replacement["old_text"].as_str() {
                Some(t) => t,
                None => {
                    return ToolResult {
                        output: format!(
                            "Successfully performed {} replacements before error\nDetails:\n{}",
                            idx,
                            replacement_stats
                                .iter()
                                .map(|s| format!("  - {}", s))
                                .collect::<Vec<_>>()
                                .join("\n")
                        ),
                        error: Some(format!(
                            "Replacement #{}: Missing old_text parameter",
                            idx + 1
                        )),
                    };
                }
            };

            let new_text = replacement["new_text"].as_str().unwrap_or("");
            let use_regex = replacement["use_regex"].as_bool().unwrap_or(false);

            let (new_content, count) = if use_regex {
                // 使用正则表达式替换
                #[cfg(feature = "regex")]
                {
                    match regex::Regex::new(old_text) {
                        Ok(re) => {
                            let matches: Vec<_> = re.find_iter(&current_content).collect();
                            let count = matches.len();
                            let replaced = re.replace_all(&current_content, new_text).to_string();
                            (replaced, count)
                        }
                        Err(e) => {
                            return ToolResult {
                                output: format!(
                                    "Successfully performed {} replacements before error\nDetails:\n{}",
                                    idx,
                                    replacement_stats
                                        .iter()
                                        .map(|s| format!("  - {}", s))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                ),
                                error: Some(format!(
                                    "Replacement #{}: Invalid regex pattern '{}': {}",
                                    idx + 1,
                                    old_text,
                                    e
                                )),
                            };
                        }
                    }
                }
                #[cfg(not(feature = "regex"))]
                {
                    // 如果没有启用regex特性，降级为简单字符串替换
                    let count = current_content.matches(old_text).count();
                    let replaced = current_content.replace(old_text, new_text);
                    (replaced, count)
                }
            } else {
                // 简单字符串替换
                let count = current_content.matches(old_text).count();
                let replaced = current_content.replace(old_text, new_text);
                (replaced, count)
            };

            current_content = new_content;
            replacement_stats.push(format!(
                "Replacement #{}: '{}' -> '{}' ({} occurrences, regex: {})",
                idx + 1,
                if old_text.len() > 50 {
                    &old_text[..50]
                } else {
                    old_text
                },
                if new_text.len() > 50 {
                    &new_text[..50]
                } else {
                    new_text
                },
                count,
                use_regex
            ));
        }

        // 写回文件
        if let Err(e) = fs::write(path, current_content).await {
            return ToolResult {
                output: format!(
                    "Successfully performed {} replacements but failed to write file\nDetails:\n{}",
                    replacement_stats.len(),
                    replacement_stats.join("\n")
                ),
                error: Some(format!("Failed to write file: {}", e)),
            };
        }

        // 返回成功结果
        let output = format!(
            "Successfully performed {} replacements on file: {}\n\nDetails:\n{}",
            replacement_stats.len(),
            file_path,
            replacement_stats.join("\n")
        );

        ToolResult {
            output,
            error: None,
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
