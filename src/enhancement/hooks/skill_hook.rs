use std::sync::Arc;
use crate::base::tool::GetSkillDetailTool;
use crate::enhancement::hooks::{AgentHook, HookScope, InitContext};
use crate::manager::SkillManager;
use async_trait::async_trait;
#[cfg(feature = "logging")]
use serde_json::json;
#[cfg(feature = "logging")]
use crate::debug_log;

/// 技能钩子
/// 自动为Agent添加可用技能列表和get_skill_detail工具
#[derive(Debug)]
pub struct SkillHook {
    #[allow(dead_code)] // 在on_init方法中使用
    skill_manager: Arc<SkillManager>,
    #[allow(dead_code)] // 公共API，为将来扩展预留
    scope: HookScope,
}

impl SkillHook {
    pub fn new(skill_manager: Arc<SkillManager>) -> Self {
        Self { 
            skill_manager,
            scope: HookScope::default(),  // 对所有Agent生效
        }
    }
    
    /// 创建带作用范围的SkillHook
    #[allow(dead_code)] // 公共API，为将来扩展预留
    pub fn with_scope(skill_manager: Arc<SkillManager>, scope: HookScope) -> Self {
        Self { skill_manager, scope }
    }
}

#[async_trait]
impl AgentHook for SkillHook {
    fn name(&self) -> &str {
        "skill_hook"
    }
    
    fn scope(&self) -> &HookScope {
        &self.scope
    }
    
    async fn on_init(&self, ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        // 1. 在system_prompt中添加可用技能列表
        let skills_list = self.skill_manager.list_all().await;
        
        #[cfg(feature = "logging")]
        {
            debug_log!(
                "DEBUG",
                &ctx.base.session_id,
                &ctx.base.request_id,
                &ctx.base.span_id,
                &format!("skill_hook.rs:{}", line!()),
                json!({
                    "event": "skill_hook_init_start",
                    "agent_name": ctx.base.agent_name,
                    "skills_count": skills_list.len()
                })
            );
        }
        
        if !skills_list.is_empty() {
            let skills_info = format!(
                "\n\n## Available Skills\n\nYou have access to the following skills:\n{}\n\nUse the 'get_skill_detail' tool to view the full content of any skill when needed.",
                skills_list.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
            );
            ctx.agent_spec.system_prompt.push_str(&skills_info);
        }
        
        // 2. 添加 get_skill_detail 工具
        let has_get_skill = ctx.agent_spec.tools.iter().any(|t| t.name() == "get_skill_detail");
        if !has_get_skill {
            let get_skill_tool = Arc::new(GetSkillDetailTool::new(
                self.skill_manager.clone()
            ));
            ctx.agent_spec.tools.push(get_skill_tool);
            
            #[cfg(feature = "logging")]
            {
                debug_log!(
                    "DEBUG",
                    &ctx.base.session_id,
                    &ctx.base.request_id,
                    &ctx.base.span_id,
                    &format!("skill_hook.rs:{}", line!()),
                    json!({
                        "event": "skill_hook_tool_added",
                        "agent_name": ctx.base.agent_name,
                        "tool_name": "get_skill_detail"
                    })
                );
            }
        }
        
        #[cfg(feature = "logging")]
        {
            debug_log!(
                "DEBUG",
                &ctx.base.session_id,
                &ctx.base.request_id,
                &ctx.base.span_id,
                &format!("skill_hook.rs:{}", line!()),
                json!({
                    "event": "skill_hook_init_complete",
                    "agent_name": ctx.base.agent_name,
                    "added_tool": !has_get_skill
                })
            );
        }
        
        Ok(())
    }
}
