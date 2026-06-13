use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolApprovalType, ToolPreCheckResult, ToolResult};
use serde_json::{Value as JsonValue, json};
use walkdir::WalkDir;

// 过滤类型枚举
#[derive(Debug, Clone)]
enum FilterType {
    All,       // 显示所有
    DirsOnly,  // 仅文件夹
    FilesOnly, // 仅文件
}

// ====================== 目录树工具实现 ======================
#[derive(Debug, Clone, Default)]
pub struct DirectoryTreeTool;

#[async_trait]
impl Tool for DirectoryTreeTool {
    fn name(&self) -> &str {
        "directory_tree"
    }

    fn description(&self) -> &str {
        "遍历目录生成文件树，可指定路径和最大深度，标注文件/文件夹类型。默认深度为1以防止数据过多，可根据需要调整max_depth（建议不超过5层）。默认忽略.git、.idea等隐藏文件夹以防止污染，如需查看请使用show_hidden参数。支持通过filter_type过滤只显示文件夹或文件"
    }

    fn pre_check(&self, input: &JsonValue) -> Option<ToolPreCheckResult> {
        let path = input["path"].as_str().unwrap_or_default().to_string();
        Some(ToolPreCheckResult {
            approval_type: ToolApprovalType::Path,
            parameters: json!({ "path": path }),
        })
    }

    /// JSON 参数 schema：path(必选), max_depth(可选，默认1，最大10), show_hidden(可选，默认false), filter_type(可选，默认all)
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
                    "description": "最大遍历深度，默认1，最大10，建议不超过5以避免输出过大",
                    "default": 1,
                    "maximum": 10
                },
                "show_hidden": {
                    "type": "boolean",
                    "description": "是否显示隐藏文件和文件夹（如.git、.idea），默认false",
                    "default": false
                },
                "filter_type": {
                    "type": "string",
                    "enum": ["all", "dirs_only", "files_only"],
                    "description": "过滤类型：all=全部，dirs_only=仅文件夹，files_only=仅文件",
                    "default": "all"
                },
                "extensions": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "文件后缀过滤列表，如[\".rs\", \".java\"]，仅在files_only或all模式下对文件生效",
                    "examples": [[".rs", ".java", ".toml"]]
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

        let max_depth = input.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize; // 默认值为 3

        // 限制最大深度为 10
        let max_depth = max_depth.min(10);

        // 解析show_hidden参数，默认false
        let show_hidden = input
            .get("show_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 解析filter_type参数，默认all
        let filter_type_str = input
            .get("filter_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let filter_type = match filter_type_str {
            "dirs_only" => FilterType::DirsOnly,
            "files_only" => FilterType::FilesOnly,
            _ => FilterType::All,
        };

        // 解析extensions参数，支持文件后缀过滤
        let extensions: Vec<String> = input
            .get("extensions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        // 生成文件树
        let tree = match generate_tree(path, max_depth, show_hidden, filter_type, &extensions) {
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
fn generate_tree(
    root: &str,
    max_depth: usize,
    show_hidden: bool,
    filter_type: FilterType,
    extensions: &[String],
) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    lines.push(format!(" {} (目录)", root));

    // walkdir 遍历：高性能、流式、低内存
    let walker = WalkDir::new(root)
        .max_depth(max_depth + 1) // walkdir 包含自身，+1 对齐语义
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(|e| {
            // 如果show_hidden为false，过滤掉隐藏的目录
            if !show_hidden && let Some(name) = e.file_name().to_str() {
                // 忽略常见的版本控制和IDE目录
                if name == ".git"
                    || name == ".idea"
                    || name == ".vscode"
                    || name == "node_modules"
                    || name == "__pycache__"
                {
                    return false;
                }
                // 忽略以.开头的隐藏文件或目录
                if name.starts_with('.') {
                    return false;
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

        // 根据filter_type过滤
        match filter_type {
            FilterType::DirsOnly if !entry.file_type().is_dir() => continue,
            FilterType::FilesOnly if entry.file_type().is_dir() => continue,
            _ => {}
        }

        // 根据extensions过滤文件后缀
        if !extensions.is_empty() && entry.file_type().is_file() {
            if let Some(file_name) = entry.file_name().to_str() {
                let has_matching_ext = extensions
                    .iter()
                    .any(|ext| file_name.to_lowercase().ends_with(ext));
                if !has_matching_ext {
                    continue;
                }
            } else {
                // 无法转换为字符串的文件名，跳过
                continue;
            }
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
