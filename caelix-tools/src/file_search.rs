use async_trait::async_trait;
use caelix_api::tool::Tool;
use caelix_api::tool::{ToolPreCheckResult, ToolResult};
use serde_json::{Value as JsonValue, json};
use std::fs::read_to_string;
use std::sync::OnceLock;
use tokio::process::Command;
use tokio::sync::Mutex;
use walkdir::WalkDir;

use crate::security::require_path_allowed;

const MAX_NATIVE_SEARCH_FILE_SIZE: u64 = 2 * 1024 * 1024;

// ====================== 智能搜索工具（零新增依赖） ======================
/// 全局文件搜索：优先使用ripgrep，无rg时自动降级为原生搜索
#[derive(Debug, Default, Clone)]
pub struct SmartSearchTool;

// 缓存 ripgrep 检测结果，使用 Mutex 避免阻塞 tokio runtime
static HAS_RIPGREP: OnceLock<Mutex<Option<bool>>> = OnceLock::new();

fn has_ripgrep_mutex() -> &'static Mutex<Option<bool>> {
    HAS_RIPGREP.get_or_init(|| Mutex::new(None))
}

impl SmartSearchTool {
    /// 异步检测 ripgrep 是否可用（避免阻塞 runtime）
    async fn has_ripgrep(&self) -> bool {
        let mut guard = has_ripgrep_mutex().lock().await;
        if let Some(cached) = *guard {
            return cached;
        }

        let result = Command::new("rg")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok();

        *guard = Some(result);
        result
    }

    /// 方案1：调用系统 rg 命令行（高性能）
    async fn search_with_ripgrep(&self, input: &JsonValue) -> ToolResult {
        let path = input["path"].as_str().unwrap_or_default();
        let keyword = input["keyword"].as_str().unwrap_or_default();
        let search_filename = input["search_type"].as_str() == Some("filename");
        let empty = vec![];
        let modes = input["modes"].as_array().unwrap_or(&empty);

        // 基础参数校验
        if path.is_empty() || keyword.is_empty() {
            return ToolResult {
                output: String::new(),
                error: Some("参数错误：path 和 keyword 不能为空".to_string()),
            };
        }

        let modes = match validate_modes(modes) {
            Ok(modes) => modes,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(e),
                };
            }
        };

        let mut cmd = Command::new("rg");

        // ✅ 修复：rg 命令顺序必须是：rg [选项] 关键词 路径
        // 先加所有模式参数
        for mode in modes {
            cmd.arg(mode);
        }

        // 文件名搜索模式
        if search_filename {
            cmd.arg("--files-with-matches");
        }

        // 终止 rg 选项解析，避免 keyword/path 以 '-' 开头时被当作开关。
        cmd.arg("--").arg(keyword).arg(path);

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
                };
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() {
            ToolResult {
                output: stdout,
                error: if stderr.is_empty() {
                    None
                } else {
                    Some(stderr)
                },
            }
        } else {
            ToolResult {
                output: stdout,
                error: Some(format!("搜索执行失败：{}", stderr.trim())),
            }
        }
    }

    /// 方案2：原生降级搜索（纯标准库，无 regex，无依赖）
    fn search_native(input: JsonValue) -> ToolResult {
        let path = input["path"].as_str().unwrap_or_default();
        let keyword = input["keyword"].as_str().unwrap_or_default();
        let search_filename = input["search_type"].as_str() == Some("filename");
        let empty_modes = vec![];

        let modes = input["modes"].as_array().unwrap_or(&empty_modes);
        if let Err(e) = validate_modes(modes) {
            return ToolResult {
                output: String::new(),
                error: Some(e),
            };
        }

        let ignore_case = modes.iter().any(|m| m == "-i" || m == "--ignore-case");
        let show_line_num = modes.iter().any(|m| m == "-n" || m == "--line-number");

        if path.is_empty() || keyword.is_empty() {
            return ToolResult {
                output: String::new(),
                error: Some("参数错误：path 和 keyword 不能为空".to_string()),
            };
        }

        let keyword = if ignore_case {
            keyword.to_lowercase()
        } else {
            keyword.to_string()
        };

        let mut output = String::new();

        // 遍历目录，跳过错误
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let file_path = entry.path();
            if !file_path.is_file() {
                continue;
            }

            // 文件名搜索
            if search_filename {
                let Some(name) = file_path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };

                let matched = if ignore_case {
                    name.to_lowercase().contains(&keyword)
                } else {
                    name.contains(&keyword)
                };

                if matched {
                    output.push_str(&format!("{}\n", file_path.display()));
                }
                continue;
            }

            if let Ok(meta) = file_path.metadata()
                && meta.len() > MAX_NATIVE_SEARCH_FILE_SIZE
            {
                continue;
            }

            // 内容搜索：跳过无法读取的文件
            let content = match read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // 逐行匹配
            for (idx, line) in content.lines().enumerate() {
                let line_match = if ignore_case {
                    line.to_lowercase().contains(&keyword)
                } else {
                    line.contains(&keyword)
                };

                if line_match {
                    if show_line_num {
                        output.push_str(&format!(
                            "{}:{}: {}\n",
                            file_path.display(),
                            idx + 1,
                            line
                        ));
                    } else {
                        output.push_str(&format!("{}: {}\n", file_path.display(), line));
                    }
                }
            }
        }

        ToolResult {
            output,
            error: None,
        }
    }
}

#[async_trait]
impl Tool for SmartSearchTool {
    fn name(&self) -> &str {
        "global_file_search"
    }

    fn pre_check(&self, input: &JsonValue) -> Option<ToolPreCheckResult> {
        let path = input["path"].as_str().unwrap_or_default().to_string();
        Some(ToolPreCheckResult::path(path))
    }

    fn description(&self) -> &str {
        "全局文件搜索工具，支持忽略大小写、显示行号、内容/文件名搜索"
    }

    /// 完整版AI工具提示
    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["path", "keyword"],
            "description": "全局文件搜索",
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
        let path = input["path"].as_str().unwrap_or_default();
        if let Err(e) = require_path_allowed(path).await {
            return ToolResult {
                output: String::new(),
                error: Some(format!("Path rejected by security policy: {}", e)),
            };
        }

        if self.has_ripgrep().await {
            self.search_with_ripgrep(&input).await
        } else {
            // ✅ 修复：阻塞任务 + 所有权传递，无生命周期错误
            match tokio::task::spawn_blocking(move || Self::search_native(input)).await {
                Ok(res) => res,
                Err(e) => ToolResult {
                    output: String::new(),
                    error: Some(format!("原生搜索执行异常：{}", e)),
                },
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

fn validate_modes(modes: &[JsonValue]) -> Result<Vec<&'static str>, String> {
    let mut out = Vec::new();
    for mode in modes {
        match mode.as_str().unwrap_or_default() {
            "" => {}
            "-i" | "--ignore-case" => out.push("--ignore-case"),
            "-n" | "--line-number" => out.push("--line-number"),
            other => {
                return Err(format!(
                    "Unsupported search mode '{}'. Allowed modes: -i/--ignore-case, -n/--line-number",
                    other
                ));
            }
        }
    }

    Ok(out)
}
