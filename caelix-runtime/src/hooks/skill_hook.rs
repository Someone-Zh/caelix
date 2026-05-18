use std::sync::Arc;
// TODO: GetSkillDetailTool 在 caelix-tools 中，但由于循环依赖暂不使用
// use caelix_api::tool::GetSkillDetailTool;
use crate::hooks::{AgentHook, HookScope, InitContext, HookCapability};
// TODO: SkillManager 将在 caelix-config 中定义，这里暂时使用占位符
// use caelix_config::managers::SkillManager;
use async_trait::async_trait;
#[cfg(feature = "logging")]
use serde_json::json;
#[cfg(feature = "logging")]
use crate::debug_log;

/// 技能钩子
/// 自动为Agent添加可用技能列表和get_skill_detail工具
/// 
/// 注意：由于循环依赖问题，此钩子将在 caelix-config 中重新实现
#[derive(Debug)]
pub struct SkillHook {
    // TODO: 恢复 skill_manager 字段
    // skill_manager: Arc<SkillManager>,
    #[allow(dead_code)] // 公共API，为将来扩展预留
    scope: HookScope,
}

impl SkillHook {
    // TODO: 恢复构造函数
    /*
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
    */
}

#[async_trait]
impl AgentHook for SkillHook {
    fn name(&self) -> &str {
        "skill_hook"
    }
    
    fn capabilities(&self) -> HookCapability {
        // SkillHook只关心Init阶段
        HookCapability::INIT
    }
    
    fn scope(&self) -> &HookScope {
        &self.scope
    }
    
    async fn on_init(&self, _ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        // TODO: 恢复技能钩子逻辑
        // 由于循环依赖，此功能将在 caelix-config 中实现
        Ok(())
    }
}
