use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::base::tool::{Tool, ToolResult};

#[derive(Debug, Clone)]
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read file content (full/partial line range), show line numbers, or only get file size (1-based line numbers)"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path of the file to read"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Start line number (1-based, default: 1)",
                    "minimum": 1
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line number (1-based, default: last line of file)",
                    "minimum": 1
                },
                "show_line_numbers": {
                    "type": "boolean",
                    "description": "Show line numbers in output (default: true)",
                    "default": true
                },
                "only_size": {
                    "type": "boolean",
                    "description": "Only return file size in bytes (skip reading content, default: false)",
                    "default": false
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 1. 解析必选参数：文件路径
        let file_path = match input["file_path"].as_str() {
            Some(v) => v,
            None => return ToolResult {
                output: String::new(),
                error: Some("Missing required parameter: file_path".to_string()),
            },
        };

        // 2. 解析新增参数
        let only_size = input["only_size"].as_bool().unwrap_or(false);
        let show_line_numbers = input["show_line_numbers"].as_bool().unwrap_or(true);

        let path = Path::new(file_path);

        // ==============================================
        // 🔥 模式 1：仅获取文件大小（O(1)，极快，不读文件）
        // ==============================================
        if only_size {
            match tokio::fs::metadata(path).await {
                Ok(meta) => {
                    let size = meta.len();
                    return ToolResult {
                        output: format!("File: {}\nSize: {} bytes", file_path, size),
                        error: None,
                    };
                }
                Err(e) => return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to get file size: {}", e)),
                },
            }
        }

        // ==============================================
        // 模式 2：读取文件内容（原有高性能逻辑）
        // ==============================================
        let start_line = input["start_line"].as_u64().unwrap_or(1) as usize;
        let end_line = input["end_line"].as_u64().map(|v| v as usize);

        // 行号合法性校验
        if end_line.map_or(false, |end| end < start_line) {
            return ToolResult {
                output: String::new(),
                error: Some("end_line cannot be less than start_line".to_string()),
            };
        }

        // 异步流式打开文件
        let file = match File::open(path).await {
            Ok(f) => f,
            Err(e) => return ToolResult {
                output: String::new(),
                error: Some(format!("Failed to open file: {}", e)),
            },
        };

        // 流式逐行读取
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut result_lines = Vec::new();
        let mut current_line = 0;

        loop {
            current_line += 1;

            // 提前终止：超过结束行直接停止
            if let Some(end) = end_line {
                if current_line > end {
                    break;
                }
            }

            match lines.next_line().await {
                Ok(Some(line)) => {
                    if current_line >= start_line {
                        // 根据配置决定是否显示行号
                        if show_line_numbers {
                            result_lines.push(format!("{}: {}", current_line, line));
                        } else {
                            result_lines.push(line);
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read line {}: {}", current_line, e)),
                },
            }
        }

        // 行号越界检查
        if current_line < start_line {
            return ToolResult {
                output: String::new(),
                error: Some(format!(
                    "File only has {} lines, start_line {} is out of range",
                    current_line - 1,
                    start_line
                )),
            };
        }

        // 构建输出
        let output = format!(
            "Successfully read file: {}\nLines {}-{}:\n{}",
            file_path,
            start_line,
            end_line.unwrap_or(current_line - 1),
            result_lines.join("\n")
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
