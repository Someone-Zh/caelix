use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use walkdir::WalkDir;
use crate::base::tool::{ToolResult,Tool};


// ====================== 目录树工具实现 ======================
#[derive(Debug, Clone, Default)]
pub struct DirectoryTreeTool;

#[async_trait]
impl Tool for DirectoryTreeTool {
    fn name(&self) -> &str {
        "directory_tree"
    }

    fn description(&self) -> &str {
        "遍历目录生成文件树，可指定路径和最大深度，标注文件/文件夹类型。默认忽略.git、.idea等隐藏文件夹以防止污染，如需查看请使用show_hidden参数"
    }

    /// JSON 参数 schema：path(必选), max_depth(可选，默认3，最大10), show_hidden(可选，默认false)
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要遍历的根路径"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "最大遍历深度，默认3，最大10",
                    "default": 3,
                    "maximum": 10
                },
                "show_hidden": {
                    "type": "boolean",
                    "description": "是否显示隐藏文件和文件夹（如.git、.idea），默认false",
                    "default": false
                }
            },
            "required": ["path"]
        })
    }

    /// 执行遍历并返回树形字符串
    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 解析参数
        let path = match input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("缺少参数：path".into()),
                };
            }
        };

        let max_depth = input
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize; // 默认值为 3

        // 限制最大深度为 10
        let max_depth = max_depth.min(10);
        
        // 解析show_hidden参数，默认false
        let show_hidden = input
            .get("show_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 生成文件树
        let tree = match generate_tree(path, max_depth, show_hidden) {
            Ok(t) => t,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("生成目录树失败：{}", e)),
                };
            }
        };

        ToolResult {
            output: tree,
            error: None,
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

// ====================== 核心：walkdir 生成树形结构 ======================
fn generate_tree(root: &str, max_depth: usize, show_hidden: bool) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!(" {} (目录)", root));

    // walkdir 遍历：高性能、流式、低内存
    let walker = WalkDir::new(root)
        .max_depth(max_depth + 1) // walkdir 包含自身，+1 对齐语义
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(|e| {
            // 如果show_hidden为false，过滤掉隐藏的目录
            if !show_hidden {
                if let Some(name) = e.file_name().to_str() {
                    // 忽略常见的版本控制和IDE目录
                    if name == ".git" || name == ".idea" || name == ".vscode" 
                       || name == "node_modules" || name == "__pycache__" {
                        return false;
                    }
                    // 忽略以.开头的隐藏文件或目录
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        });

    for entry in walker {
        let entry = entry?;
        let depth = entry.depth();

        // 跳过根目录自身
        if depth == 0 {
            continue;
        }

        // 构建前缀缩进
        let prefix = "│  ".repeat(depth - 1);
        let icon = if entry.file_type().is_dir() {
            "├─ 目录"
        } else {
            "├─ 文件"
        };

        let name = entry.file_name().to_string_lossy();
        lines.push(format!("{prefix}{icon} {}", name));
    }

    Ok(lines.join("\n"))
}
