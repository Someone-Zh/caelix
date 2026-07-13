//! 会话管理业务逻辑

use super::{validate_session_id, summarize_first_message, LIST_SESSIONS_MAX_CONCURRENCY};
use crate::types::SessionSummary;
use caelix_api::error::ApiError;
use caelix_api::message::AgentMessage;
use caelix_api::provider::SessionUsageView;
use caelix_api::{AgentRunManagerTrait, ContextProvider};
use caelix_runtime::context::CaelixContext;
use futures::{StreamExt, stream};

pub(crate) async fn create_session(ctx: &CaelixContext) -> Result<String, ApiError> {
    let session_id = caelix_api::utils::generate_session_id();
    ctx.session_manager
        .create_session_config(session_id.clone())
        .await
        .map_err(|e| ApiError::InternalError(format!("创建会话配置失败: {}", e)))?;
    Ok(session_id)
}

pub(crate) async fn create_session_with_id(
    ctx: &CaelixContext,
    session_id: String,
) -> Result<(), ApiError> {
    validate_session_id(&session_id)?;
    ctx.session_manager
        .create_session_config(session_id)
        .await
        .map_err(|e| ApiError::InternalError(format!("创建会话配置失败: {}", e)))?;
    Ok(())
}

pub(crate) async fn session_exists(ctx: &CaelixContext, session_id: &str) -> Result<bool, ApiError> {
    validate_session_id(session_id)?;
    Ok(ctx.session_manager.session_exists(session_id).await)
}

pub(crate) async fn list_sessions(ctx: &CaelixContext) -> Result<Vec<SessionSummary>, ApiError> {
    let session_ids = ctx.session_manager.list_sessions().await;
    let session_manager = ctx.session_manager.clone();

    let mut summaries: Vec<SessionSummary> = Vec::with_capacity(session_ids.len());
    let mut stream = stream::iter(session_ids.into_iter().map(|session_id| {
        let sm = session_manager.clone();
        async move {
            let config = sm.get_session_config(&session_id).await?;
            let messages = sm.get_session_messages(&session_id).await.unwrap_or_default();
            let first_msgs: Vec<_> = messages.into_iter().take(1).collect();
            let summary = summarize_first_message(&first_msgs);
            Some(SessionSummary {
                session_id,
                created_at: config.created_at,
                summary,
            })
        }
    }))
    .buffer_unordered(LIST_SESSIONS_MAX_CONCURRENCY);

    while let Some(summary) = stream.next().await {
        if let Some(s) = summary {
            summaries.push(s);
        }
    }

    Ok(summaries)
}

pub(crate) async fn get_session_messages(
    ctx: &CaelixContext,
    session_id: &str,
) -> Result<Vec<AgentMessage>, ApiError> {
    validate_session_id(session_id)?;

    if !ctx.session_manager.session_exists(session_id).await {
        return Err(ApiError::session_not_found(session_id));
    }

    ctx.session_manager
        .get_session_messages(session_id)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn set_session_provider(
    ctx: &CaelixContext,
    session_id: &str,
    provider: &str,
) -> Result<(), ApiError> {
    validate_session_id(session_id)?;

    let provider_manager = ctx.llm_provider_manager.read().await;
    if provider_manager.get_provider(provider).is_none() {
        return Err(ApiError::provider_not_found(provider));
    }

    ctx.session_manager
        .set_session_provider(session_id, provider)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn set_session_model(
    ctx: &CaelixContext,
    session_id: &str,
    model: &str,
) -> Result<(), ApiError> {
    validate_session_id(session_id)?;

    let session_config = ctx.session_manager.get_session_config(session_id).await;
    let model_valid = match session_config {
        Some(ref config) if config.provider.is_some() => {
            let provider_name = config.provider.as_deref().unwrap();
            let provider_manager = ctx.llm_provider_manager.read().await;
            if let Some(provider) = provider_manager.get_provider(provider_name) {
                let pconfig = provider.config();
                pconfig.models.values().any(|m| m == model)
                    || pconfig.default_model.as_deref() == Some(model)
            } else {
                false
            }
        }
        _ => {
            let provider_manager = ctx.llm_provider_manager.read().await;
            provider_manager.get_all_providers().iter().any(|(_, p)| {
                let pconfig = p.config();
                pconfig.models.values().any(|m| m == model)
                    || pconfig.default_model.as_deref() == Some(model)
            })
        }
    };

    if !model_valid {
        return Err(ApiError::model_not_found(model));
    }

    ctx.session_manager
        .set_session_model(session_id, model)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))
}

pub(crate) async fn get_session_usage(
    ctx: &CaelixContext,
    session_id: &str,
) -> Result<Option<SessionUsageView>, ApiError> {
    validate_session_id(session_id)?;

    let tracker = ctx
        .usage_tracker()
        .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;

    let session_provider = ctx
        .session_manager
        .get_session_config(session_id)
        .await
        .and_then(|cfg| cfg.provider);

    let ctx_window_tokens = {
        let provider_manager = ctx.llm_provider_manager.read().await;
        if let Some(ref prov_name) = session_provider {
            provider_manager
                .get_provider(prov_name)
                .and_then(|p| p.config().ctx_window_tokens)
        } else {
            provider_manager
                .get_all_providers()
                .first()
                .and_then(|(_, p)| p.config().ctx_window_tokens)
        }
    };

    Ok(tracker.snapshot_session(session_id, ctx_window_tokens).await)
}

pub(crate) async fn stop_agent(ctx: &CaelixContext, session_id: &str) -> Result<bool, ApiError> {
    validate_session_id(session_id)?;
    Ok(ctx.agent_run_manager.stop_agent(session_id).await)
}
