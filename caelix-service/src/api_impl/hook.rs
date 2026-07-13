//! Hook 管理业务逻辑

use crate::types::HookInfo;
use caelix_api::error::ApiError;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn list_hooks(ctx: &CaelixContext) -> Result<Vec<HookInfo>, ApiError> {
    let hooks = ctx.hook_registry.list_hooks().await;
    let infos: Vec<HookInfo> = hooks
        .into_iter()
        .map(|hook| HookInfo {
            name: hook.name().to_string(),
            capabilities: format!("{:?}", hook.capabilities()),
            scope: format!("{:?}", hook.scope()),
        })
        .collect();
    Ok(infos)
}

pub(crate) async fn get_hook_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<HookInfo>, ApiError> {
    let hooks = ctx.hook_registry.list_hooks().await;
    if let Some(hook) = hooks.into_iter().find(|h| h.name() == name) {
        Ok(Some(HookInfo {
            name: hook.name().to_string(),
            capabilities: format!("{:?}", hook.capabilities()),
            scope: format!("{:?}", hook.scope()),
        }))
    } else {
        Ok(None)
    }
}
