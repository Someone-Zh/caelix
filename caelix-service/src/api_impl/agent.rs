//! 智能体配置业务逻辑

use crate::types::{AgentSpecInfo, ProviderInfo};
use caelix_api::error::ApiError;
use caelix_api::provider::LlmProvider;
use caelix_runtime::context::CaelixContext;

pub(crate) fn get_default_provider(ctx: &CaelixContext) -> Option<String> {
    ctx.llm_provider_manager
        .try_read()
        .ok()
        .and_then(|pm| pm.get_all_providers().first().map(|(n, _)| n.clone()))
}

pub(crate) fn get_default_model(ctx: &CaelixContext) -> Option<String> {
    ctx.llm_provider_manager
        .try_read()
        .ok()
        .and_then(|pm| {
            pm.get_all_providers()
                .first()
                .and_then(|(_, p)| {
                    let config = p.config();
                    config
                        .default_model
                        .clone()
                        .or_else(|| config.models.values().next().cloned())
                })
        })
}

pub(crate) async fn get_providers(ctx: &CaelixContext) -> Result<Vec<ProviderInfo>, ApiError> {
    let providers: Vec<(String, std::sync::Arc<dyn LlmProvider>)> = {
        let provider_manager = ctx.llm_provider_manager.read().await;
        provider_manager.get_all_providers()
    };

    let result = providers
        .into_iter()
        .map(|(name, provider)| {
            let config = provider.config();
            let llm_type = match config.llm_type {
                caelix_api::provider::LlmType::OpenAI => "openai".to_string(),
            };
            let models: Vec<String> = config.models.values().cloned().collect();
            ProviderInfo {
                name,
                llm_type,
                models,
            }
        })
        .collect();

    Ok(result)
}

pub(crate) async fn get_provider_models(
    ctx: &CaelixContext,
    provider_name: &str,
) -> Result<Vec<String>, ApiError> {
    let provider_manager = ctx.llm_provider_manager.read().await;

    let provider = provider_manager
        .get_provider(provider_name)
        .ok_or_else(|| ApiError::provider_not_found(provider_name))?;

    let config = provider.config();
    let models: Vec<String> = config.models.values().cloned().collect();

    Ok(models)
}

pub(crate) async fn list_agents(ctx: &CaelixContext) -> Vec<String> {
    let agents = ctx.agent_manager.get_all().await;
    agents.iter().map(|a| a.get_spec().name.clone()).collect()
}

pub(crate) async fn list_agents_info(
    ctx: &CaelixContext,
) -> Result<Vec<AgentSpecInfo>, ApiError> {
    let agents = ctx.agent_manager.get_all().await;
    let infos: Vec<AgentSpecInfo> = agents
        .into_iter()
        .map(|agent| {
            let spec = agent.get_spec();
            AgentSpecInfo {
                name: spec.name.clone(),
                group: spec.group.as_ref().map(|g| g.as_str().to_string()),
                tools: spec.tools.iter().map(|t| t.name().to_string()).collect(),
            }
        })
        .collect();
    Ok(infos)
}

pub(crate) async fn get_agent_info(
    ctx: &CaelixContext,
    name: &str,
) -> Result<Option<AgentSpecInfo>, ApiError> {
    if let Some(agent) = ctx.agent_manager.get(name).await {
        let spec = agent.get_spec();
        Ok(Some(AgentSpecInfo {
            name: spec.name.clone(),
            group: spec.group.as_ref().map(|g| g.as_str().to_string()),
            tools: spec.tools.iter().map(|t| t.name().to_string()).collect(),
        }))
    } else {
        Ok(None)
    }
}
