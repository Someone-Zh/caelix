#![cfg(feature = "ast")]

use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolPreCheckResult, ToolResult};
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use tree_sitter::{Language, Node, Parser, Tree};

static LANG_REGISTRY: LazyLock<HashMap<&'static str, Language>> = LazyLock::new(|| {
    let mut m: HashMap<&'static str, Language> = HashMap::new();
    #[cfg(feature = "ast-rust")]
    m.insert("rs", tree_sitter_rust::LANGUAGE.into());
    #[cfg(feature = "ast-python")]
    m.insert("py", tree_sitter_python::LANGUAGE.into());
    #[cfg(feature = "ast-ts")]
    {
        m.insert("js", tree_sitter_javascript::LANGUAGE.into());
        m.insert("jsx", tree_sitter_javascript::LANGUAGE.into());
        m.insert("ts", tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        m.insert("tsx", tree_sitter_typescript::LANGUAGE_TSX.into());
    }
    #[cfg(feature = "ast-go")]
    m.insert("go", tree_sitter_go::LANGUAGE.into());
    #[cfg(feature = "ast-c")]
    m.insert("c", tree_sitter_c::LANGUAGE.into());
    m
});

fn get_language(ext: &str) -> Option<&Language> {
    LANG_REGISTRY.get(ext)
}

fn symbol_kinds(lang: &str, kind_filter: &str) -> &'static [&'static str] {
    match (lang, kind_filter) {
        (_, "all") => match lang {
            "rs" => &["function_item", "method_definition", "struct_item", "enum_item", "trait_item", "impl_item"],
            "py" => &["function_definition", "class_definition"],
            "js" | "jsx" | "ts" | "tsx" => &["function_declaration", "method_definition", "arrow_function", "class_declaration", "struct_expression"],
            "go" => &["function_declaration", "method_declaration", "type_declaration"],
            "c" => &["function_definition", "struct_specifier", "enum_specifier"],
            _ => &[],
        },
        (_, "function") => match lang {
            "rs" => &["function_item"],
            "py" => &["function_definition"],
            "js" | "jsx" | "ts" | "tsx" => &["function_declaration", "arrow_function"],
            "go" => &["function_declaration"],
            "c" => &["function_definition"],
            _ => &[],
        },
        (_, "method") => match lang {
            "rs" => &["method_definition"],
            "py" => &["function_definition"],
            "js" | "jsx" | "ts" | "tsx" => &["method_definition"],
            "go" => &["method_declaration"],
            "c" => &[],
            _ => &[],
        },
        (_, "struct") => match lang {
            "rs" => &["struct_item"],
            "py" => &[],
            "js" | "jsx" | "ts" | "tsx" => &["struct_expression"],
            "go" => &["type_declaration"],
            "c" => &["struct_specifier"],
            _ => &[],
        },
        (_, "enum") => match lang {
            "rs" => &["enum_item"],
            "py" => &[],
            "js" | "jsx" | "ts" | "tsx" => &[],
            "go" => &[],
            "c" => &["enum_specifier"],
            _ => &[],
        },
        (_, "class") => match lang {
            "rs" => &[],
            "py" => &["class_definition"],
            "js" | "jsx" | "ts" | "tsx" => &["class_declaration"],
            "go" => &[],
            "c" => &[],
            _ => &[],
        },
        _ => &[],
    }
}

fn get_node_name(source: &str, node: &Node) -> String {
    let mut child = node.child(0);
    while let Some(c) = child {
        if c.is_named() {
            match c.kind() {
                "identifier" | "type_identifier" | "field_identifier" | "property_identifier" => {
                    return c.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                }
                _ => {}
            }
        }
        child = c.next_sibling();
    }
    String::new()
}

fn get_signature(source: &str, node: &Node) -> String {
    let start = node.start_byte() as usize;
    let end = node.end_byte() as usize;
    let text = &source[start..end];
    text.lines().next().unwrap_or(text).trim().to_string()
}

fn collect_symbols(source: &str, tree: &Tree, kinds: &[&str]) -> Vec<JsonValue> {
    let root = tree.root_node();
    let mut symbols = Vec::new();
    let mut cursor = root.walk();

    loop {
        let node = cursor.node();
        if kinds.contains(&node.kind()) {
            let name = get_node_name(source, &node);
            if !name.is_empty() {
                let start_line = node.start_position().row + 1;
                let end_line = node.end_position().row + 1;
                let signature = get_signature(source, &node);
                symbols.push(json!({
                    "name": name,
                    "kind": node.kind(),
                    "start_line": start_line,
                    "end_line": end_line,
                    "signature": signature,
                }));
            }
        }

        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return symbols;
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ListSymbolsTool;

#[async_trait]
impl Tool for ListSymbolsTool {
    fn name(&self) -> &str {
        "list_symbols"
    }

    fn description(&self) -> &str {
        "基于 AST 列出文件中的符号（函数/结构体/枚举/类/方法）"
    }

    fn pre_check(&self, input: &JsonValue) -> Option<ToolPreCheckResult> {
        Some(ToolPreCheckResult::path(input["file_path"].as_str()?))
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path of the file to analyze"
                },
                "kind": {
                    "type": "string",
                    "enum": ["function", "struct", "enum", "class", "method", "all"],
                    "default": "all",
                    "description": "Symbol kind to filter (default: all)"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let file_path = match input["file_path"].as_str() {
            Some(v) => v,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: file_path".to_string()),
                };
            }
        };

        let kind_filter = input["kind"].as_str().unwrap_or("all");

        let ext = match Path::new(file_path).extension() {
            Some(e) => e.to_string_lossy(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("File has no extension, cannot determine language".to_string()),
                };
            }
        };

        let lang = match get_language(&ext) {
            Some(l) => l,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Unsupported language for extension '{}'", ext)),
                };
            }
        };

        let source = match tokio::fs::read_to_string(file_path).await {
            Ok(s) => s,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read file: {}", e)),
                };
            }
        };

        let max_size = 1 * 1024 * 1024;
        if source.len() > max_size {
            return ToolResult {
                output: String::new(),
                error: Some(format!("File size {} bytes exceeds limit of {} bytes", source.len(), max_size)),
            };
        }

        let mut parser = Parser::new();
        if let Err(e) = parser.set_language(lang) {
            return ToolResult {
                output: String::new(),
                error: Some(format!("Failed to set language: {}", e)),
            };
        }

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Failed to parse file".to_string()),
                };
            }
        };

        let kinds = symbol_kinds(&ext, kind_filter);
        let symbols = collect_symbols(&source, &tree, kinds);

        let output = json!({
            "file_path": file_path,
            "language": ext,
            "symbols": symbols,
        })
        .to_string();

        ToolResult { output, error: None }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Default, Clone)]
pub struct GetSymbolDefinitionTool;

#[async_trait]
impl Tool for GetSymbolDefinitionTool {
    fn name(&self) -> &str {
        "get_symbol_definition"
    }

    fn description(&self) -> &str {
        "获取指定符号的完整源码片段 + 行号范围"
    }

    fn pre_check(&self, input: &JsonValue) -> Option<ToolPreCheckResult> {
        Some(ToolPreCheckResult::path(input["file_path"].as_str()?))
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["file_path", "symbol_name"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path of the file to search"
                },
                "symbol_name": {
                    "type": "string",
                    "description": "Name of the symbol to find"
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let file_path = match input["file_path"].as_str() {
            Some(v) => v,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: file_path".to_string()),
                };
            }
        };

        let symbol_name = match input["symbol_name"].as_str() {
            Some(v) => v,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: symbol_name".to_string()),
                };
            }
        };

        let ext = match Path::new(file_path).extension() {
            Some(e) => e.to_string_lossy(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("File has no extension, cannot determine language".to_string()),
                };
            }
        };

        let lang = match get_language(&ext) {
            Some(l) => l,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Unsupported language for extension '{}'", ext)),
                };
            }
        };

        let source = match tokio::fs::read_to_string(file_path).await {
            Ok(s) => s,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read file: {}", e)),
                };
            }
        };

        let max_size = 1 * 1024 * 1024;
        if source.len() > max_size {
            return ToolResult {
                output: String::new(),
                error: Some(format!("File size {} bytes exceeds limit of {} bytes", source.len(), max_size)),
            };
        }

        let mut parser = Parser::new();
        if let Err(e) = parser.set_language(lang) {
            return ToolResult {
                output: String::new(),
                error: Some(format!("Failed to set language: {}", e)),
            };
        }

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Failed to parse file".to_string()),
                };
            }
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let kinds = symbol_kinds(&ext, "all");
        let mut found = None;

        loop {
            let node = cursor.node();
            if kinds.contains(&node.kind()) {
                let name = get_node_name(&source, &node);
                if name == symbol_name {
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;
                    let start_byte = node.start_byte() as usize;
                    let end_byte = node.end_byte() as usize;
                    let content = &source[start_byte..end_byte];

                    found = Some(json!({
                        "name": name,
                        "kind": node.kind(),
                        "start_line": start_line,
                        "end_line": end_line,
                        "source": content,
                    }));
                    break;
                }
            }

            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }

        match found {
            Some(def) => ToolResult {
                output: def.to_string(),
                error: None,
            },
            None => ToolResult {
                output: String::new(),
                error: Some(format!("Symbol '{}' not found in file", symbol_name)),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}