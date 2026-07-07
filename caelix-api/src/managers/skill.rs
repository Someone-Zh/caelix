//! SkillManager - 技能管理器

use crate::plugins::SkillDef;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// 技能内脚本工具定义（对应 .skill 文件 YAML 头中 `inline_tools` 的一条）
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct InlineToolDef {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 待执行的脚本（运行时以技能文件所在目录为 CWD）
    pub script: String,
    /// 超时秒数
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

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
    /// .skill 文件的绝对路径；技能内脚本/资源以此为基准解析（`file_path.parent()` 即技能目录）
    pub file_path: PathBuf,
    /// 技能版本
    pub version: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 标签
    pub tags: Vec<String>,
    /// 本技能希望 Agent 拥有的全局工具名（从系统工具池中选取）
    pub requires_tools: Vec<String>,
    /// 本技能自带的本地脚本工具定义
    pub inline_tools: Vec<InlineToolDef>,
}

impl Skill {
    /// 内部构建：派生 `full_name` 并组装结构
    fn build(
        name: String,
        namespace: String,
        description: String,
        content: String,
        file_path: PathBuf,
        version: Option<String>,
        author: Option<String>,
        tags: Vec<String>,
        requires_tools: Vec<String>,
        inline_tools: Vec<InlineToolDef>,
    ) -> Self {
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
            file_path,
            version,
            author,
            tags,
            requires_tools,
            inline_tools,
        }
    }

    /// 创建新的技能（元数据字段填默认值）
    pub fn new(
        name: String,
        namespace: String,
        description: String,
        content: String,
        file_path: PathBuf,
    ) -> Self {
        Self::build(
            name,
            namespace,
            description,
            content,
            file_path,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    /// 创建带完整 YAML 元数据的技能
    #[allow(clippy::too_many_arguments)]
    pub fn with_metadata(
        name: String,
        namespace: String,
        description: String,
        content: String,
        file_path: PathBuf,
        version: Option<String>,
        author: Option<String>,
        tags: Vec<String>,
        requires_tools: Vec<String>,
        inline_tools: Vec<InlineToolDef>,
    ) -> Self {
        Self::build(
            name,
            namespace,
            description,
            content,
            file_path,
            version,
            author,
            tags,
            requires_tools,
            inline_tools,
        )
    }
}

impl From<SkillDef> for Skill {
    fn from(def: SkillDef) -> Self {
        Self::build(
            def.name,
            def.namespace,
            def.description,
            def.content,
            def.file_path,
            def.version,
            def.author,
            def.tags,
            def.requires_tools,
            def.inline_tools,
        )
    }
}

/// 技能注册中心错误
#[derive(Debug, Error)]
pub enum SkillRegistryError {
    #[error("Skill with name '{0}' already exists")]
    SkillAlreadyExists(String),
    #[error("Failed to load skill: {0}")]
    LoadError(String),
}

/// 技能管理器，负责维护所有技能的索引
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
            return Err(SkillRegistryError::SkillAlreadyExists(
                skill.full_name.clone(),
            ));
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

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}
