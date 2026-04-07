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
        "遍历目录生成文件树，可指定路径和最大深度，标注文件/文件夹类型"
    }

    /// JSON 参数 schema：path(必选), max_depth(可选，默认0=当前目录)
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
                    "description": "最大遍历深度，0=仅当前目录，默认0"
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
            .unwrap_or(0) as usize;

        // 生成文件树
        let tree = match generate_tree(path, max_depth) {
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
fn generate_tree(root: &str, max_depth: usize) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("📂 {} (目录)", root));

    // walkdir 遍历：高性能、流式、低内存
    let walker = WalkDir::new(root)
        .max_depth(max_depth + 1) // walkdir 包含自身，+1 对齐语义
        .sort_by(|a, b| a.file_name().cmp(b.file_name()));

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
            "├─ 📂 目录"
        } else {
            "├─ 📄 文件"
        };

        let name = entry.file_name().to_string_lossy();
        lines.push(format!("{prefix}{icon} {}", name));
    }

    Ok(lines.join("\n"))
}