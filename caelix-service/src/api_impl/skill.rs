//! 技能管理业务逻辑

use crate::types::{SkillInfo, SkillTriggerInfo};
use caelix_api::error::ApiError;
use caelix_api::ContextProvider;
use caelix_runtime::context::CaelixContext;
use std::sync::Arc;

fn skill_to_info(skill: &Arc<caelix_api::managers::Skill>) -> SkillInfo {
    SkillInfo {
        name: skill.name.clone(),
        namespace: skill.namespace.clone(),
        full_name: skill.full_name.clone(),
        description: skill.description.clone(),
        version: skill.version.clone(),
        author: skill.author.clone(),
        tags: skill.tags.clone(),
        triggers: skill
            .triggers
            .iter()
            .map(|t| SkillTriggerInfo {
                trigger_type: t.trigger_type.clone(),
                name: t.name.clone(),
            })
            .collect(),
        globs: skill.globs.clone(),
        disable_model_invocation: skill.disable_model_invocation,
        user_invocable: skill.user_invocable,
        argument_hint: skill.argument_hint.clone(),
        compatibility: skill.compatibility.clone(),
    }
}

pub(crate) async fn list_skills(ctx: &CaelixContext) -> Result<Vec<SkillInfo>, ApiError> {
    let skills = ctx.skill_manager.get_all().await;
    Ok(skills.iter().map(skill_to_info).collect())
}

pub(crate) async fn list_skill_names(ctx: &CaelixContext) -> Result<Vec<String>, ApiError> {
    Ok(ctx.skill_manager.list_all().await)
}

pub(crate) async fn get_skill_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<SkillInfo>, ApiError> {
    if let Some(skill) = ctx.skill_manager.get(name).await {
        Ok(Some(skill_to_info(&skill)))
    } else {
        Ok(None)
    }
}

pub(crate) async fn list_project_skills(
    ctx: &CaelixContext,
    work_dir: &str,
) -> Result<Vec<SkillInfo>, ApiError> {
    let work_dir_path = std::path::Path::new(work_dir);
    let overlay = ctx.config_overlay();
    if let Err(e) = overlay.ensure_project_config_loaded(work_dir_path).await {
        tracing::warn!(error = %e, "Failed to load project config");
    }

    let configs = overlay.project_configs().await;
    if let Some(config) = configs.get(work_dir_path) {
        Ok(config.skills.values().map(skill_to_info).collect())
    } else {
        Ok(Vec::new())
    }
}
