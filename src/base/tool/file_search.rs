use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::fs::read_to_string;
use tokio::process::Command;
use walkdir::WalkDir;
use crate::base::tool::ToolResult;
use crate::base::tool::Tool;

// ====================== 智能搜索工具（零新增依赖） ======================
/// 全局文件搜索：优先使用ripgrep，无rg时自动降级为原生搜索
#[derive(Debug, Default, Clone)]
pub struct SmartSearchTool;

impl SmartSearchTool {
    /// 标准库检查 rg 是否存在（无依赖）
    async fn has_ripgrep(&self) -> bool {
        Command::new("rg")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok()
    }

    /// 方案1：调用系统 rg 命令行（高性能）
    async fn search_with_ripgrep(&self, input: &JsonValue) -> ToolResult {
        let path = input["path"].as_str().unwrap_or_default();
        let keyword = input["keyword"].as_str().unwrap_or_default();
        let search_filename = input["search_type"].as_str() == Some("filename");
        let empty = vec![];
        let modes = input["modes"].as_array().unwrap_or(&empty);
        if path.is_empty() || keyword.is_empty() {
            return ToolResult {
                output: String::new(),
                error: Some("参数错误：path 和 keyword 不能为空".to_string()),
            };
        }

        let mut cmd = Command::new("rg");
        cmd.arg(keyword).arg(path).arg("--recursive");
        for m in modes { cmd.arg(m.as_str().unwrap_or_default()); }
        if search_filename { cmd.arg("--files-with-matches"); }

        let output = match cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
        {
            Ok(o) => o,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("启动 rg 失败：{}", e)),
                }
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            ToolResult {
                output: stdout,
                error: if stderr.is_empty() { None } else { Some(stderr) },
            }
        } else {
            ToolResult {
                output: stdout,
                error: Some(format!("搜索失败：{}", stderr)),
            }
        }
    }

    /// 方案2：原生降级搜索（纯标准库，无 regex，无依赖）
    fn search_native(&self, input: &JsonValue) -> ToolResult {
        let path = input["path"].as_str().unwrap_or_default();
        let keyword = input["keyword"].as_str().unwrap_or_default();
        let search_filename = input["search_type"].as_str() == Some("filename");
        let empty_modes = vec![];

        let modes = input["modes"].as_array().unwrap_or(&empty_modes);
        let ignore_case = modes.iter().any(|m| m == "-i" || m == "--ignore-case");
        let show_line_num = modes.iter().any(|m| m == "-n" || m == "--line-number");

        if path.is_empty() || keyword.is_empty() {
            return ToolResult {
                output: String::new(),
                error: Some("参数错误：path 和 keyword 不能为空".to_string()),
            };
        }

        let keyword_lower = if ignore_case { keyword.to_lowercase() } else { keyword.to_string() };
        let mut output = String::new();

        // 使用你已有的 walkdir
        for entry in WalkDir::new(path).follow_links(false).into_iter() {
            let entry = match entry { Ok(e) => e, Err(_) => continue };
            let file_path = entry.path();
            if !file_path.is_file() { continue; }

            // 文件名搜索
            if search_filename {
                if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
                    let matched = if ignore_case {
                        name.to_lowercase().contains(&keyword_lower)
                    } else {
                        name.contains(keyword)
                    };
                    if matched {
                        output.push_str(&format!("{}\n", file_path.display()));
                    }
                }
                continue;
            }

            // 内容搜索
            let content = match read_to_string(file_path) { Ok(c) => c, Err(_) => continue };
            for (line_num, line) in content.lines().enumerate() {
                let matched = if ignore_case {
                    line.to_lowercase().contains(&keyword_lower)
                } else {
                    line.contains(keyword)
                };

                if matched {
                    if show_line_num {
                        output.push_str(&format!("{}:{}: {}\n", file_path.display(), line_num + 1, line));
                    } else {
                        output.push_str(&format!("{}: {}\n", file_path.display(), line));
                    }
                }
            }
        }

        ToolResult { output, error: None }
    }
}

#[async_trait]
impl Tool for SmartSearchTool {
    fn name(&self) -> &str {
        "global_file_search"
    }

    fn description(&self) -> &str {
        "全局文件搜索工具，优先使用系统 ripgrep(rg)，未安装时自动降级；支持忽略大小写、显示行号、内容/文件名搜索"
    }

    /// 完整版AI工具提示
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["path", "keyword"],
            "description": "全局文件搜索，自动降级",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "搜索目录路径",
                    "examples": ["./", "/Users/project"]
                },
                "keyword": {
                    "type": "string",
                    "description": "搜索关键词，rg模式下支持正则，原生模式支持普通匹配",
                    "examples": ["async fn", "TODO", "error"]
                },
                "modes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "-i=忽略大小写 -n=显示行号",
                    "examples": [["-i", "-n"]]
                },
                "search_type": {
                    "type": "string",
                    "enum": ["content", "filename"],
                    "default": "content",
                    "description": "content=搜索内容 filename=搜索文件名"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        if self.has_ripgrep().await {
            self.search_with_ripgrep(&input).await
        } else {
            let this = self.clone();
            let result = tokio::task::spawn_blocking(move || this.search_native(&input)).await;

            match result {
                Ok(res) => res,
                Err(e) => ToolResult {
                    output: String::new(),
                    error: Some(format!("原生搜索异常：{}", e)),
                },
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}