use std::sync::Arc;
use crate::base::tool::GetSkillDetailTool;
use crate::enhancement::hooks::AgentHook;
use crate::base::agent::AgentSpec;
use crate::manager::SkillManager;

/// 技能钩子
/// 自动为Agent添加可用技能列表和get_skill_detail工具
#[derive(Debug)]
pub struct SkillHook {
    #[allow(dead_code)] // 在trait方法中使用
    skill_manager: Arc<SkillManager>,
}

impl SkillHook {
    pub fn new(skill_manager: Arc<SkillManager>) -> Self {
        Self { skill_manager }
    }
}

impl AgentHook for SkillHook {
    fn name(&self) -> &str {
        "skill_hook"
    }

    fn enhance_agent(&self, agent_spec: &mut AgentSpec) {
        // 1. 在system_prompt中添加可用技能列表
        let skills_list = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.skill_manager.list_all())
        });
        
        if !skills_list.is_empty() {
            let skills_info = format!(
                "\n\n## Available Skills\n\nYou have access to the following skills:\n{}\n\nUse the 'get_skill_detail' tool to view the full content of any skill when needed.",
                skills_list.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
            );
            agent_spec.system_prompt.push_str(&skills_info);
        }
        
        // 2. 添加 get_skill_detail 工具
        // 检查是否已经添加过该工具
        let has_get_skill = agent_spec.tools.iter().any(|t| t.name() == "get_skill_detail");
        if !has_get_skill {
            let get_skill_tool = Arc::new(GetSkillDetailTool::new(
                self.skill_manager.clone()
            ));
            agent_spec.tools.push(get_skill_tool);
        }
    }
}
