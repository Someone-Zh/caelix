use crate::enhancement::commands::{Command, CommandType};
use crate::config::skills_loader::parse_yaml_markdown_file;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// 命令配置的 YAML 部分
#[derive(Debug, Deserialize)]
struct CommandConfig {
    name: String,
    description: String,
    #[serde(default)]
    r#type: Option<String>,  // "prompt" 或 "shell",默认为 prompt
}

/// 从单个 .md 文件加载命令
fn load_command_from_file(file_path: &Path) -> Result<Command, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;
    
    let (config, markdown_content) = parse_yaml_markdown_file::<CommandConfig>(&content)?;
    
    // 解析命令类型,默认为 Prompt
    let cmd_type = match config.r#type.as_deref() {
        Some("shell") => CommandType::Shell,
        _ => CommandType::Prompt,  // 默认
    };
    
    Ok(Command::new(
        config.name,
        config.description,
        cmd_type,
        markdown_content,
    ))
}

/// 递归扫描目录并加载所有 .md 文件
pub async fn load_commands_from_directory(directory_path: &str) -> Result<Vec<Command>, String> {
    let dir_path = Path::new(directory_path);
    
    if !dir_path.exists() {
        return Err(format!("Directory does not exist: {}", directory_path));
    }
    
    if !dir_path.is_dir() {
        return Err(format!("Path is not a directory: {}", directory_path));
    }
    
    let mut commands = Vec::new();
    
    // 递归遍历目录
    for entry in walkdir::WalkDir::new(dir_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        
        // 只处理 .md 文件
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            match load_command_from_file(path) {
                Ok(command) => {
                    println!("Loaded command: {} ({})", command.name, 
                        match command.r#type {
                            CommandType::Prompt => "prompt",
                            CommandType::Shell => "shell",
                        }
                    );
                    commands.push(command);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load command from {:?}: {}", path, e);
                }
            }
        }
    }
    
    Ok(commands)
}

/// 注册所有命令到 CommandManager
pub async fn register_all_commands(
    directory_path: &str,
    command_manager: &crate::manager::CommandManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let commands = load_commands_from_directory(directory_path).await?;
    command_manager.register_batch(commands).await;
    Ok(())
}
