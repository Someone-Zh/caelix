use crate::manager::{Skill, SkillRegistryError};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// 通用的 YAML + Markdown 文件解析函数
/// 处理 `---YAML---Markdown` 格式
pub fn parse_yaml_markdown_file<T: for<'de> Deserialize<'de>>(
    content: &str,
) -> Result<(T, String), String> {
    // 查找第一个 ---
    let first_delimiter = content
        .find("---")
        .ok_or("Invalid file format: missing opening ---")?;
    
    // 查找第二个 ---（从第一个之后开始）
    let second_delimiter = content[first_delimiter + 3..]
        .find("---")
        .ok_or("Invalid file format: missing closing ---")?
        + first_delimiter + 3;
    
    // 提取 YAML 部分
    let yaml_content = &content[first_delimiter + 3..second_delimiter];
    
    // 提取 Markdown 部分（content）
    let markdown_content = content[second_delimiter + 3..].trim().to_string();
    
    // 解析 YAML
    let config: T = serde_yaml::from_str(yaml_content)
        .map_err(|e| format!("Failed to parse YAML: {}", e))?;
    
    Ok((config, markdown_content))
}

/// Skill 配置的 YAML 部分
#[derive(Debug, Deserialize)]
struct SkillConfig {
    name: String,
    description: String,
}

/// 递归扫描目录并加载所有 .skill 文件
pub async fn load_skills_from_directory(directory_path: &str) -> Result<Vec<Skill>, String> {
    let dir_path = Path::new(directory_path);
    
    if !dir_path.exists() {
        return Err(format!("Directory does not exist: {}", directory_path));
    }
    
    if !dir_path.is_dir() {
        return Err(format!("Path is not a directory: {}", directory_path));
    }
    
    let mut skills = Vec::new();
    load_skills_recursive(dir_path, dir_path, &mut skills).await?;
    
    Ok(skills)
}

/// 递归加载技能的辅助函数
async fn load_skills_recursive(
    base_dir: &Path,
    current_dir: &Path,
    skills: &mut Vec<Skill>,
) -> Result<(), String> {
    use futures::future::BoxFuture;
    
    let mut entries = Vec::new();
    for entry in fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {:?}: {}", current_dir, e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        entries.push(entry.path());
    }
    
    for path in entries {
        if path.is_dir() {
            // 递归处理子目录
            Box::pin(load_skills_recursive(base_dir, &path, skills)).await?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("skill") {
            // 处理 .skill 文件
            println!("Loading skill from: {:?}", path);
            match load_single_skill(&path, base_dir).await {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    eprintln!("Warning: Failed to load skill from {:?}: {}", path, e);
                }
            }
        }
    }
    
    Ok(())
}

/// 从单个 .skill 文件加载技能
async fn load_single_skill(file_path: &Path, base_dir: &Path) -> Result<Skill, String> {
    // 读取文件内容
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;
    
    // 解析文件
    let (config, skill_content) = parse_yaml_markdown_file::<SkillConfig>(&content)?;
    
    // 计算命名空间
    let relative_path = file_path
        .strip_prefix(base_dir)
        .map_err(|e| format!("Failed to compute relative path: {}", e))?;
    
    let namespace = relative_path
        .parent()
        .map(|p| p.to_string_lossy().replace("/", "::").replace("\\", "::"))
        .unwrap_or_default();
    
    let name = relative_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("Invalid file name: {:?}", file_path))?;
    
    // 创建 Skill 对象
    let skill = Skill::new(name, namespace, config.description, skill_content);
    
    Ok(skill)
}

/// 注册所有技能到管理器（从指定目录加载）
pub async fn register_all_skills(
    directory_path: &str,
    skill_manager: &crate::manager::SkillManager,
) -> Result<(), SkillRegistryError> {
    // 从指定目录加载所有 skill
    let skills = load_skills_from_directory(directory_path)
        .await
        .map_err(|e| SkillRegistryError::LoadError(e))?;
    
    // 注册所有加载的 skill
    for skill in skills {
        skill_manager.register(skill).await?;
    }
    
    Ok(())
}
