use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use serde_json::json;
use super::Tool;
use crate::base::AgentError;
use serde::{Deserialize, Serialize};
use std::fs;

// 文件写入工具（包括创建路径）
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(FileWriteTool)
    }
}

// 文件读取工具
#[derive(Debug, Clone)]
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
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(FileReadTool)
    }
}

// 文件修改工具
#[derive(Debug, Clone)]
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
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(FileModifyTool)
    }
}

// 文件列表工具
#[derive(Debug, Clone)]
pub struct FileListTool;

impl FileListTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories in a folder with depth parameter. Default depth is 1."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the directory to list"
                },
                "depth": {
                    "type": "integer",
                    "description": "The depth to traverse, default is 1",
                    "default": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AgentError> {
        let path = input["path"].as_str().unwrap_or(".");
        let depth = input["depth"].as_i64().unwrap_or(1) as usize;

        let entries = list_directory_contents(path, depth)?;

        Ok(json!({
            "status": "success",
            "path": path,
            "depth": depth,
            "entries": entries
        }))
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(FileListTool)
    }
}

// 辅助函数：递归列出目录内容
fn list_directory_contents(path: &str, max_depth: usize) -> Result<Vec<serde_json::Value>, AgentError> {
    let mut entries = Vec::new();
    
    if max_depth == 0 {
        return Ok(entries);
    }
    
    let dir_entries = fs::read_dir(path).map_err(|e| AgentError::ToolError(format!("Failed to read directory {}: {}", path, e)))?;
    
    for entry in dir_entries {
        let entry = entry.map_err(|e| AgentError::ToolError(format!("Failed to read entry: {}", e)))?;
        let file_type = entry.file_type().map_err(|e| AgentError::ToolError(format!("Failed to get file type: {}", e)))?;
        let file_name = entry.file_name();
        
        let entry_info = if file_type.is_file() {
            json!({
                "name": file_name.to_string_lossy(),
                "type": "file",
                "path": entry.path().to_string_lossy()
            })
        } else if file_type.is_dir() {
            let sub_entries = list_directory_contents(
                &entry.path().to_string_lossy(), 
                max_depth - 1
            )?;
            json!({
                "name": file_name.to_string_lossy(),
                "type": "directory",
                "path": entry.path().to_string_lossy(),
                "contents": sub_entries
            })
        } else {
            json!({
                "name": file_name.to_string_lossy(),
                "type": "other",
                "path": entry.path().to_string_lossy()
            })
        };
        
        entries.push(entry_info);
    }
    
    Ok(entries)
}