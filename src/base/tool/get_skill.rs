use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use crate::base::tool::{Tool, ToolResult};
use crate::manager::SkillManager;

/// 获取技能详情工具
#[derive(Debug)]
pub struct GetSkillDetailTool {
    skill_manager: Arc<SkillManager>,
}

impl GetSkillDetailTool {
    pub fn new(skill_manager: Arc<SkillManager>) -> Self {
        Self { skill_manager }
    }
}

#[async_trait]
impl Tool for GetSkillDetailTool {
    fn name(&self) -> &str {
        "get_skill_detail"
    }

    fn description(&self) -> &str {
        "获取指定技能的详细内容。参数为技能的完整名称(包含命名空间),例如 'coding::git' 或 'writing::email'。使用 'list_skills' 查看所有可用技能。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "技能的完整名称(包含命名空间),例如 'coding::git'"
                }
            },
            "required": ["skill_name"]
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 提取 skill_name 参数
        let skill_name = match input.get("skill_name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: skill_name".to_string()),
                };
            }
        };

        // 从 SkillManager 获取技能
        match self.skill_manager.get(&skill_name).await {
            Some(skill) => {
                let result = format!(
                    "# {}\n\n**Namespace:** {}\n**Full Name:** {}\n\n**Description:**\n{}\n\n**Content:**\n\n{}",
                    skill.name,
                    if skill.namespace.is_empty() {
                        "(root)".to_string()
                    } else {
                        skill.namespace.clone()
                    },
                    skill.full_name,
                    skill.description,
                    skill.content
                );
                
                ToolResult {
                    output: result,
                    error: None,
                }
            }
            None => {
                // 列出所有可用技能以帮助调试
                let all_skills = self.skill_manager.list_all().await;
                let skills_list = if all_skills.is_empty() {
                    "No skills available.".to_string()
                } else {
                    format!("Available skills:\n{}", all_skills.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n"))
                };
                
                ToolResult {
                    output: String::new(),
                    error: Some(format!(
                        "Skill '{}' not found.\n\n{}",
                        skill_name,
                        skills_list
                    )),
                }
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}

impl Clone for GetSkillDetailTool {
    fn clone(&self) -> Self {
        Self {
            skill_manager: self.skill_manager.clone(),
        }
    }
}
