use crate::manager::{AgentRegistryError, ToolManager};
use crate::base::agent::AgentSpec;
use crate::config::CaelixContext;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Agent 配置的 YAML 部分
#[derive(Debug, Deserialize)]
struct AgentConfig {
    name: String,
    tools: Vec<String>,
}

/// 解析 .agent 文件内容
/// 格式：YAML 头（--- ... ---）+ Markdown 体
fn parse_agent_file(content: &str) -> Result<(AgentConfig, String), String> {
    // 查找第一个 ---
    let first_delimiter = content
        .find("---")
        .ok_or("Invalid .agent file: missing opening ---")?;
    
    // 查找第二个 ---（从第一个之后开始）
    let second_delimiter = content[first_delimiter + 3..]
        .find("---")
        .ok_or("Invalid .agent file: missing closing ---")?
        + first_delimiter + 3;
    
    // 提取 YAML 部分
    let yaml_content = &content[first_delimiter + 3..second_delimiter];
    
    // 提取 Markdown 部分（system_prompt）
    let system_prompt = content[second_delimiter + 3..].trim().to_string();
    
    // 解析 YAML
    let config: AgentConfig = serde_yaml::from_str(yaml_content)
        .map_err(|e| format!("Failed to parse YAML: {}", e))?;
    
    Ok((config, system_prompt))
}

/// 从单个 .agent 文件创建 AgentSpec
async fn create_agent_from_file(
    file_path: &Path,
    tool_manager: &ToolManager,
    context: &CaelixContext,
) -> Result<AgentSpec, String> {
    // 读取文件内容
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;
    
    // 解析文件
    let (config, system_prompt) = parse_agent_file(&content)?;
    
    // 根据工具名称列表获取工具实例
    let mut tools = Vec::new();
    for tool_name in &config.tools {
        let tool = tool_manager.get(tool_name).await
            .ok_or_else(|| format!("Tool '{}' not found in ToolManager", tool_name))?;
        tools.push(tool);
    }
    
    // 特殊处理：如果工具列表中包含 "delegate_task"，需要额外创建
    if config.tools.iter().any(|t| t == "delegate_task") {
        // 检查是否已经添加过 delegate_task
        let has_delegate = tools.iter().any(|t| t.name() == "delegate_task");
        if !has_delegate {
            let delegate_task_tool = crate::config::tools_loader::create_delegate_task_tool(
                Arc::new(context.clone()),
                None,
                None,
            );
            tools.push(delegate_task_tool);
        }
    }
    
    Ok(AgentSpec::new(config.name, system_prompt, tools))
}

/// 从指定目录加载所有 .agent 文件
pub async fn load_agents_from_directory(
    directory_path: &str,
    tool_manager: &ToolManager,
    context: &CaelixContext,
) -> Result<Vec<AgentSpec>, String> {
    let dir_path = Path::new(directory_path);
    
    if !dir_path.exists() {
        return Err(format!("Directory does not exist: {}", directory_path));
    }
    
    if !dir_path.is_dir() {
        return Err(format!("Path is not a directory: {}", directory_path));
    }
    
    let mut agents = Vec::new();
    
    // 遍历目录中的所有 .agent 文件
    for entry in fs::read_dir(dir_path)
        .map_err(|e| format!("Failed to read directory {}: {}", directory_path, e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        
        // 只处理 .agent 文件
        if path.extension().and_then(|ext| ext.to_str()) == Some("agent") {
            println!("Loading agent from: {:?}", path);
            match create_agent_from_file(&path, tool_manager, context).await {
                Ok(agent) => agents.push(agent),
                Err(e) => {
                    eprintln!("Warning: Failed to load agent from {:?}: {}", path, e);
                }
            }
        }
    }
    
    Ok(agents)
}

/// 注册所有角色智能体到注册中心（从指定目录加载）
pub async fn register_all_agents(context: &CaelixContext, directory_path: &str) -> Result<(), AgentRegistryError> {
    let agent_manager = context.agent_manager.clone();
    let tool_manager = context.tool_manager.clone();
    
    // 从指定目录加载所有 agent
    let agents = load_agents_from_directory(directory_path, &tool_manager, context)
        .await
        .map_err(|e| AgentRegistryError::LoadError(e))?;
    
    // 注册所有加载的 agent
    for agent in agents {
        agent_manager.register(agent).await?;
    }
    
    Ok(())
}