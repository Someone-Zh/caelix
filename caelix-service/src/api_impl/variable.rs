//! 变量管理业务逻辑

use crate::variable_replacer::VariableReplacer;
use caelix_api::error::ApiError;
use caelix_runtime::context::CaelixContext;
use std::collections::HashMap;

pub(crate) async fn set_variable(
    ctx: &CaelixContext,
    key: &str,
    value: &str,
) -> Result<(), ApiError> {
    ctx.variable_manager.set_global(key, value).await;
    Ok(())
}

pub(crate) async fn get_variable(
    ctx: &CaelixContext,
    key: &str,
) -> Result<Option<String>, ApiError> {
    Ok(ctx.variable_manager.get_global(key).await)
}

pub(crate) async fn delete_variable(ctx: &CaelixContext, key: &str) -> Result<(), ApiError> {
    ctx.variable_manager.delete_global(key).await;
    Ok(())
}

pub(crate) async fn list_variables(
    ctx: &CaelixContext,
) -> Result<HashMap<String, String>, ApiError> {
    Ok(ctx.variable_manager.list_globals().await)
}

pub(crate) async fn set_space_variable(
    ctx: &CaelixContext,
    space: &str,
    key: &str,
    value: &str,
) -> Result<(), ApiError> {
    ctx.variable_manager.set_space_var(space, key, value).await;
    Ok(())
}

pub(crate) async fn get_space_variable(
    ctx: &CaelixContext,
    space: &str,
    key: &str,
) -> Result<Option<String>, ApiError> {
    Ok(ctx.variable_manager.get_space_var(space, key).await)
}

pub(crate) async fn delete_space_variable(
    ctx: &CaelixContext,
    space: &str,
    key: &str,
) -> Result<(), ApiError> {
    ctx.variable_manager.delete_space_var(space, key).await;
    Ok(())
}

pub(crate) async fn list_space_variables(
    ctx: &CaelixContext,
    space: &str,
) -> Result<HashMap<String, String>, ApiError> {
    Ok(ctx.variable_manager.list_space_vars(space).await)
}

pub(crate) async fn replace_variables(
    ctx: &CaelixContext,
    text: &str,
    space: Option<&str>,
) -> Result<String, ApiError> {
    let replacer = VariableReplacer::new(ctx.variable_manager.clone());
    Ok(replacer.replace_async(text, space).await)
}
