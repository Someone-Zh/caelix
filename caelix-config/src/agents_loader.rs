use crate::skills_loader::parse_yaml_markdown_file;
use caelix_api::agent::AgentSpec;
use caelix_api::managers::ToolManager;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Agent 配置的 YAML 部分
#[derive(Debug, Deserialize)]
struct AgentConfig {
    name: String,
    tools: Vec<String>,
    group: Option<String>,
}

/// 从单个 .agent 文件创建 AgentSpec
async fn create_agent_from_file(
    file_path: &Path,
    tool_manager: &ToolManager,
) -> Result<AgentSpec, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;

    let (config, system_prompt) = parse_yaml_markdown_file::<AgentConfig>(&content)?;

    let mut tools = Vec::new();
    for tool_name in &config.tools {
        let tool = tool_manager
            .get(tool_name)
            .await
            .ok_or_else(|| format!("Tool '{}' not found in ToolManager", tool_name))?;
        tools.push(tool);
    }

    Ok(AgentSpec::with_group(
        config.name,
        system_prompt,
        tools,
        config.group,
    ))
}

/// 从指定目录加载所有 .agent 文件为 AgentSpec
pub async fn load_agents_from_directory(
    directory_path: &str,
    tool_manager: &ToolManager,
) -> Result<Vec<AgentSpec>, String> {
    let dir_path = Path::new(directory_path);

    if !dir_path.exists() {
        return Err(format!("Directory does not exist: {}", directory_path));
    }

    if !dir_path.is_dir() {
        return Err(format!("Path is not a directory: {}", directory_path));
    }

    let mut agents = Vec::new();

    for entry in fs::read_dir(dir_path)
        .map_err(|e| format!("Failed to read directory {}: {}", directory_path, e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.extension().and_then(|ext| ext.to_str()) == Some("agent") {
            println!("Loading agent from: {:?}", path);
            match create_agent_from_file(&path, tool_manager).await {
                Ok(agent) => agents.push(agent),
                Err(e) => {
                    eprintln!("Warning: Failed to load agent from {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(agents)
}
