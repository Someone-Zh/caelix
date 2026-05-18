use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 技能模型
#[derive(Debug, Clone)]
pub struct Skill {
    /// 技能名称(不含命名空间)
    pub name: String,
    /// 命名空间路径(如 "coding" 或 "a::b::c")
    pub namespace: String,
    /// 完整名称(命名空间::名称,如 "coding::git")
    pub full_name: String,
    /// 技能描述
    pub description: String,
    /// 技能内容(Markdown格式)
    pub content: String,
}

impl Skill {
    /// 创建新的技能
    pub fn new(name: String, namespace: String, description: String, content: String) -> Self {
        let full_name = if namespace.is_empty() {
            name.clone()
        } else {
            format!("{}::{}", namespace, name)
        };
        
        Self {
            name,
            namespace,
            full_name,
            description,
            content,
        }
    }
}

/// 技能管理器,负责维护所有技能的索引
#[derive(Debug, Clone)]
pub struct SkillManager {
    skills: Arc<RwLock<HashMap<String, Arc<Skill>>>>,
}

impl SkillManager {
    /// 创建新的技能管理器
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册技能
    pub async fn register(&self, skill: Skill) -> Result<(), SkillRegistryError> {
        let mut skills = self.skills.write().await;
        if skills.contains_key(&skill.full_name) {
            return Err(SkillRegistryError::SkillAlreadyExists(skill.full_name));
        }
        skills.insert(skill.full_name.clone(), Arc::new(skill));
        Ok(())
    }

    /// 根据完整名称获取技能
    pub async fn get(&self, name: &str) -> Option<Arc<Skill>> {
        let skills = self.skills.read().await;
        skills.get(name).cloned()
    }

    /// 列出所有技能的完整名称
    pub async fn list_all(&self) -> Vec<String> {
        let skills = self.skills.read().await;
        let mut names: Vec<String> = skills.keys().cloned().collect();
        names.sort();
        names
    }

    /// 按命名空间列出技能
    pub async fn list_by_namespace(&self, namespace: &str) -> Vec<String> {
        let skills = self.skills.read().await;
        skills
            .keys()
            .filter(|name| {
                if namespace.is_empty() {
                    !name.contains("::")
                } else {
                    name.starts_with(&format!("{}::", namespace)) || name.as_str() == namespace
                }
            })
            .cloned()
            .collect()
    }

    /// 获取所有技能
    pub async fn get_all(&self) -> Vec<Arc<Skill>> {
        let skills = self.skills.read().await;
        skills.values().cloned().collect()
    }

    /// 移除技能
    pub async fn remove(&self, name: &str) -> Option<Arc<Skill>> {
        let mut skills = self.skills.write().await;
        skills.remove(name)
    }
}

/// 技能注册中心错误
#[derive(Debug, thiserror::Error)]
pub enum SkillRegistryError {
    #[error("Skill with name '{0}' already exists")]
    SkillAlreadyExists(String),
    #[error("Failed to load skill: {0}")]
    LoadError(String),
}
