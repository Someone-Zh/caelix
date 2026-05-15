use crate::manager::{AgentRegistryError, ToolManager};
use crate::base::agent::AgentSpec;
use crate::config::CaelixContext;
use crate::config::skills_loader::parse_yaml_markdown_file;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Agent 配置的 YAML 部分
#[derive(Debug, Deserialize)]
struct AgentConfig {
    name: String,
    tools: Vec<String>,
    group: Option<String>,  // 新增：可选的group字段
}

/// 从单个 .agent 文件创建 AgentSpec
async fn create_agent_from_file(
    file_path: &Path,
    tool_manager: &ToolManager,
    _context: &CaelixContext,
) -> Result<AgentSpec, String> {
    // 读取文件内容
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;
    
    // 解析文件
    let (config, system_prompt) = parse_yaml_markdown_file::<AgentConfig>(&content)?;
    
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
            let delegate_task_tool = crate::config::tools_loader::create_delegate_task_tool();
            tools.push(delegate_task_tool);
        }
    }
    
    // 使用 with_group 构造函数，保持向后兼容
    Ok(AgentSpec::with_group(config.name, system_prompt, tools, config.group))
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
    let hook_registry = context.hook_registry.clone();
    
    // 从指定目录加载所有 agent
    let mut agents = load_agents_from_directory(directory_path, &tool_manager, context)
        .await
        .map_err(AgentRegistryError::LoadError)?;
    
    // 对每个agent应用init-hooks进行增强
    for agent in agents.iter_mut() {
        println!("Applying init hooks to agent: {}", agent.name);
        if let Err(e) = hook_registry.apply_init_hooks(agent, None).await {
            eprintln!("Warning: Failed to apply init hooks to agent '{}': {}", agent.name, e);
            // 继续处理其他agent，不因钩子失败而中断
        }
    }
    
    // 注册所有增强后的agent
    for agent in agents {
        agent_manager.register(agent).await?;
    }
    
    Ok(())
}