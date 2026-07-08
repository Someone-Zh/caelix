use caelix_api::context::try_caelix_context;

pub async fn require_path_allowed(path: &str) -> Result<(), String> {
    let Some(ctx) = try_caelix_context() else {
        return Err("Security context is not initialized".to_string());
    };

    ctx.security_checker().check_path(path).await
}

pub async fn require_command_allowed(command: &str) -> Result<(), String> {
    let Some(ctx) = try_caelix_context() else {
        return Err("Security context is not initialized".to_string());
    };

    ctx.security_checker().check_command(command).await
}
