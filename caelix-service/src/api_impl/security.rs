//! 安全检查业务逻辑

use crate::types::SecurityCheckerInfo;
use caelix_api::error::ApiError;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn get_security_config(ctx: &CaelixContext) -> Result<SecurityCheckerInfo, ApiError> {
    let config = ctx.security_checker.get_config().await;
    Ok(SecurityCheckerInfo { config })
}

pub(crate) async fn add_path_include(ctx: &CaelixContext, path: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_path_include(path.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn add_path_exclude(ctx: &CaelixContext, path: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_path_exclude(path.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn add_url_include(ctx: &CaelixContext, pattern: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_url_include(pattern.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn add_url_exclude(ctx: &CaelixContext, pattern: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_url_exclude(pattern.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn add_command_include(ctx: &CaelixContext, command: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_command_include(command.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn add_command_exclude(ctx: &CaelixContext, command: &str) -> Result<(), ApiError> {
    ctx.security_checker
        .add_command_exclude(command.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn check_path(ctx: &CaelixContext, path: &str) -> Result<bool, ApiError> {
    Ok(ctx.security_checker.is_path_safe(path).await)
}

pub(crate) async fn check_url(ctx: &CaelixContext, url: &str) -> Result<bool, ApiError> {
    Ok(ctx.security_checker.is_url_safe(url).await)
}

pub(crate) async fn check_command(ctx: &CaelixContext, command: &str) -> Result<bool, ApiError> {
    Ok(ctx.security_checker.is_command_safe(command).await)
}
