use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use serde_json::json;
use super::Tool;

// 文件写入工具（包括创建路径）
pub struct FileWriteTool;

impl FileWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file and parent directories if they don't exist."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AgentError> {
        let file_path = input["file_path"].as_str().ok_or_else(|| AgentError::ToolError("Missing file_path parameter".to_string()))?;
        let content = input["content"].as_str().ok_or_else(|| AgentError::ToolError("Missing content parameter".to_string()))?;

        // 创建父目录
        if let Some(parent) = Path::new(file_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| AgentError::ToolError(format!("Failed to create directory: {}", e)))?;
            }
        }

        // 写入文件
        let mut file = File::create(file_path).map_err(|e| AgentError::ToolError(format!("Failed to create file: {}", e)))?;
        file.write_all(content.as_bytes()).map_err(|e| AgentError::ToolError(format!("Failed to write to file: {}", e)))?;

        Ok(json!({
            "status": "success",
            "message": format!("File written successfully: {}", file_path)
        }))
    }
}

// 文件读取工具
pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read content from a file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to read"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AgentError> {
        let file_path = input["file_path"].as_str().ok_or_else(|| AgentError::ToolError("Missing file_path parameter".to_string()))?;

        let mut file = File::open(file_path).map_err(|e| AgentError::ToolError(format!("Failed to open file: {}", e)))?;
        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| AgentError::ToolError(format!("Failed to read file: {}", e)))?;

        Ok(json!({
            "status": "success",
            "content": content
        }))
    }
}

// 文件修改工具
pub struct FileModifyTool;

impl FileModifyTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for FileModifyTool {
    fn name(&self) -> &str {
        "file_modify"
    }

    fn description(&self) -> &str {
        "Modify content of an existing file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to modify"
                },
                "content": {
                    "type": "string",
                    "description": "The new content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AgentError> {
        let file_path = input["file_path"].as_str().ok_or_else(|| AgentError::ToolError("Missing file_path parameter".to_string()))?;
        let content = input["content"].as_str().ok_or_else(|| AgentError::ToolError("Missing content parameter".to_string()))?;

        // 检查文件是否存在
        if !Path::new(file_path).exists() {
            return Err(AgentError::ToolError(format!("File does not exist: {}", file_path)));
        }

        // 写入文件
        let mut file = File::create(file_path).map_err(|e| AgentError::ToolError(format!("Failed to open file for writing: {}", e)))?;
        file.write_all(content.as_bytes()).map_err(|e| AgentError::ToolError(format!("Failed to write to file: {}", e)))?;

        Ok(json!({
            "status": "success",
            "message": format!("File modified successfully: {}", file_path)
        }))
    }
}