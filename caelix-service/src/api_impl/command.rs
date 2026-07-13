//! 命令管理业务逻辑

use crate::types::CommandInfo;
use caelix_api::error::ApiError;
use caelix_api::ContextProvider;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn list_commands(ctx: &CaelixContext) -> Result<Vec<CommandInfo>, ApiError> {
    let commands = ctx.command_manager.get_all().await;
    let infos: Vec<CommandInfo> = commands
        .into_iter()
        .map(|cmd| CommandInfo {
            name: cmd.name.clone(),
            command_type: cmd.command_type.to_string(),
            description: cmd.description.clone(),
        })
        .collect();
    Ok(infos)
}

pub(crate) async fn get_command_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<CommandInfo>, ApiError> {
    if let Some(cmd) = ctx.command_manager.get_by_name(name).await {
        Ok(Some(CommandInfo {
            name: cmd.name.clone(),
            command_type: cmd.command_type.to_string(),
            description: cmd.description.clone(),
        }))
    } else {
        Ok(None)
    }
}

pub(crate) async fn list_project_commands(
    ctx: &CaelixContext,
    work_dir: &str,
) -> Result<Vec<CommandInfo>, ApiError> {
    let work_dir_path = std::path::Path::new(work_dir);
    let overlay = ctx.config_overlay();
    if let Err(e) = overlay.ensure_project_config_loaded(work_dir_path).await {
        tracing::warn!(error = %e, "Failed to load project config");
    }

    let configs = overlay.project_configs().await;
    if let Some(config) = configs.get(work_dir_path) {
        let infos: Vec<CommandInfo> = config
            .commands
            .iter()
            .map(|cmd| CommandInfo {
                name: cmd.name.clone(),
                command_type: cmd.command_type.to_string(),
                description: cmd.description.clone(),
            })
            .collect();
        Ok(infos)
    } else {
        Ok(Vec::new())
    }
}
