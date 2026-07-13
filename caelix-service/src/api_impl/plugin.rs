//! 插件管理业务逻辑

use crate::types::PluginInfo;
use caelix_api::error::ApiError;
use caelix_api::PluginManager;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn list_plugins(ctx: &CaelixContext) -> Result<Vec<PluginInfo>, ApiError> {
    let plugins = ctx.plugin_registry.all_plugins().await;
    let infos: Vec<PluginInfo> = plugins
        .into_iter()
        .map(|plugin| PluginInfo {
            name: plugin.name().to_string(),
            capabilities: format!("{:?}", plugin.capabilities()),
        })
        .collect();
    Ok(infos)
}

pub(crate) async fn get_plugin_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<PluginInfo>, ApiError> {
    let plugins = ctx.plugin_registry.all_plugins().await;
    if let Some(plugin) = plugins.into_iter().find(|p| p.name() == name) {
        Ok(Some(PluginInfo {
            name: plugin.name().to_string(),
            capabilities: format!("{:?}", plugin.capabilities()),
        }))
    } else {
        Ok(None)
    }
}
