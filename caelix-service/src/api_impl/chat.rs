//! 聊天与对话业务逻辑

use super::{validate_session_id, make_agent_message, run_agent};
use crate::types::{ChatAsyncResult, ChatRequest};
use crate::variable_replacer::VariableReplacer;
use caelix_api::context::{ContextFutureExt, RuntimeContext};
use caelix_api::error::ApiError;
use caelix_api::message::{AgentMessage, AgentMessageType};
use caelix_api::provider::{ChatMessage, LlmConfig};
use caelix_api::{AgentRunManagerTrait, ContextProvider};
use caelix_runtime::context::CaelixContext;
use futures::{Stream, StreamExt, future, stream};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

pub(crate) async fn chat_stream_async(
    ctx: &Arc<CaelixContext>,
    request: ChatRequest,
) -> Result<ChatAsyncResult, ApiError> {
    validate_session_id(&request.session_id)?;

    // 1. 如果会话不存在则创建
    if !ctx
        .session_manager
        .session_exists(&request.session_id)
        .await
    {
        ctx.session_manager
            .create_session_config(request.session_id.clone())
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;
    }

    // 2. 生成 request_id 和 span_id
    let request_id = caelix_api::utils::generate_request_id();
    let span_id = caelix_api::utils::generate_span_id();

    // 3. 确定 provider 和 model（单次加锁）
    let (provider_name, model_name, provider) = {
        let pm = ctx.llm_provider_manager.read().await;
        let all_providers = pm.get_all_providers();
        if all_providers.is_empty() {
            return Err(ApiError::provider_not_found("（无已注册的 Provider）"));
        }

        let provider_name = request
            .provider
            .as_deref()
            .or_else(|| all_providers.first().map(|(n, _)| n.as_str()))
            .unwrap_or_default()
            .to_string();

        let provider = pm
            .get_provider(&provider_name)
            .cloned()
            .ok_or_else(|| ApiError::provider_not_found(&provider_name))?;

        let config = provider.config();
        let model_name = request
            .model
            .as_deref()
            .map(str::to_string)
            .or_else(|| config.default_model.clone())
            .or_else(|| config.models.values().next().cloned())
            .unwrap_or_default();

        (provider_name, model_name, provider)
    };

    // 4. 确定工作目录
    let work_dir: PathBuf = request
        .work_dir
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| ApiError::InternalError("无法获取工作目录".to_string()))?;

    if work_dir.as_os_str().is_empty() {
        return Err(ApiError::InternalError("工作目录不能为空".to_string()));
    }

    // 5. 确保项目配置已加载
    let context_clone = ctx.clone();
    let overlay = context_clone.config_overlay();
    if let Err(e) = overlay.ensure_project_config_loaded(&work_dir).await {
        tracing::warn!(error = %e, "Failed to load project config");
    }

    // 6. 获取 agent_spec
    let agent_name = request.agent.as_deref().unwrap_or("default");
    let agent_spec = overlay
        .get_agent_spec_for_work_dir(&work_dir, agent_name)
        .await
        .ok_or_else(|| ApiError::agent_not_found(agent_name))?;

    // 7. 构建 LlmConfig
    let config = LlmConfig {
        model_name: model_name.clone(),
    };

    // 8. 获取历史消息
    let history_messages = context_clone
        .session_manager
        .get_session_messages(&request.session_id)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    // 9. 在后台启动任务，绑定 RuntimeContext
    let debug_enabled = context_clone.env_config.debug_enabled;
    let agent_run_manager = context_clone.agent_run_manager.clone();
    let cancel_token = caelix_api::cancel::CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();
    let request_session_id = request.session_id.clone();
    let request_id_clone = request_id.clone();
    let run_id = agent_run_manager.register(request_session_id.clone(), cancel_token);

    let join_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        let _guard = caelix_runtime::agent_run_manager::RunGuard::new(
            agent_run_manager.clone(),
            request_session_id.clone(),
            run_id,
        );

        let runtime_ctx = Arc::new(RuntimeContext::new(
            Some(request_session_id.clone()),
            Some(request_id_clone.clone()),
            work_dir,
            provider_name,
            model_name,
            debug_enabled,
            cancel_token_clone,
        ));

        let ctx_for_scope = runtime_ctx.clone();

        let fut = async move {
            let mut messages: Vec<ChatMessage> = Vec::new();
            for msg in history_messages.iter() {
                if msg.r#type != AgentMessageType::Msg {
                    continue;
                }
                match serde_json::from_str::<ChatMessage>(&msg.content) {
                    Ok(chat_msg) => messages.push(chat_msg),
                    Err(e) => {
                        tracing::warn!(
                            session_id = %request_session_id,
                            error = %e,
                            content_len = msg.content.len(),
                            "Failed to deserialize history message, skipping"
                        );
                    }
                }
            }

            if let Some(original_message) = request.message.clone() {
                let space = runtime_ctx
                    .get_work_dir()
                    .to_str()
                    .map(|s| s.to_string());
                let replacer = VariableReplacer::new(context_clone.variable_manager.clone());
                let user_message = replacer
                    .replace_async(&original_message, space.as_deref())
                    .await;

                messages.push(ChatMessage::user(user_message.clone()));

                let user_msg = AgentMessage {
                    session_id: request_session_id.clone(),
                    request_id: request_id_clone.clone(),
                    span_id: runtime_ctx.get_span_id().to_string(),
                    trace_id: runtime_ctx.get_trace_id().to_string(),
                    r#type: AgentMessageType::Msg,
                    timestamp: chrono::Utc::now(),
                    content: user_message,
                    agent_name: request.agent.clone(),
                    usage: None,
                };
                if context_clone.message_bus.send_agent(user_msg).is_err() {
                    tracing::warn!("Failed to send user message to message bus");
                }
            }

            let _ = run_agent(agent_spec, messages, provider, &config)
                .await
                .inspect_err(|e| {
                    tracing::error!(
                        session_id = %request_session_id,
                        error = %e,
                        "Agent execution failed"
                    );
                });
        };

        let _ = fut.with_runtime_ctx(ctx_for_scope).await;
    });

    ctx.agent_run_manager
        .set_handles(&request.session_id, run_id, join_handle);

    Ok(ChatAsyncResult {
        request_id,
        span_id,
        session_id: request.session_id,
    })
}

pub(crate) async fn subscribe_chat_stream(
    ctx: &CaelixContext,
    session_id: &str,
) -> Result<Pin<Box<dyn Stream<Item = AgentMessage> + Send>>, ApiError> {
    validate_session_id(session_id)?;

    if !ctx.session_manager.session_exists(session_id).await {
        return Err(ApiError::session_not_found(session_id));
    }

    let (history, stream) = ctx
        .session_manager
        .subscribe_agent(session_id.to_string())
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    let history_stream = stream::iter(history);

    let session_id_owned = session_id.to_string();
    let live_stream = stream.scan(false, move |errored, r| {
        if *errored {
            return future::ready(None);
        }
        match r {
            Ok(msg) => future::ready(Some(msg)),
            Err(e) => {
                *errored = true;
                future::ready(Some(make_agent_message(
                    &session_id_owned,
                    AgentMessageType::Event,
                    format!("订阅错误: {:?}", e),
                    None,
                )))
            }
        }
    });

    let merged_stream = history_stream.chain(live_stream);

    Ok(Box::pin(merged_stream))
}

pub(crate) async fn approve_tool_call(
    ctx: &CaelixContext,
    session_id: &str,
    tool_call_id: &str,
    approved: bool,
) -> Result<(), ApiError> {
    validate_session_id(session_id)?;

    let updated_agent_msg = ctx
        .session_manager
        .update_tool_approval(session_id, tool_call_id, approved)
        .await
        .map_err(|e| ApiError::InternalError(format!("更新审批状态失败: {}", e)))?
        .ok_or_else(|| {
            ApiError::InternalError(format!(
                "未在 session {} 中找到 tool_call_id = {} 的 Assistant 消息",
                session_id, tool_call_id
            ))
        })?;

    let (_, updated_msg) = updated_agent_msg;
    let agent_name = updated_msg.agent_name.as_deref().unwrap_or("default");
    let chat_msg: ChatMessage = serde_json::from_str(&updated_msg.content)
        .map_err(|e| ApiError::InternalError(format!("反序列化 ChatMessage 失败: {}", e)))?;

    let tool_name = chat_msg
        .tool_calls
        .as_ref()
        .and_then(|tcs| tcs.iter().find(|tc| tc.id == tool_call_id))
        .map(|tc| tc.name.clone())
        .unwrap_or_default();

    if approved {
        execute_approved_tool(ctx, session_id, tool_call_id, agent_name, &chat_msg, &tool_name)
            .await?;
    } else {
        append_rejection_result(ctx, session_id, tool_call_id, &tool_name).await?;
    }

    Ok(())
}

async fn execute_approved_tool(
    ctx: &CaelixContext,
    session_id: &str,
    tool_call_id: &str,
    agent_name: &str,
    chat_msg: &ChatMessage,
    tool_name: &str,
) -> Result<(), ApiError> {
    let overlay = ctx.config_overlay();
    let agent_spec = overlay
        .get_agent_spec(agent_name)
        .await
        .ok_or_else(|| ApiError::agent_not_found(agent_name))?;

    let mut tool_result_text = String::new();
    if let Some(tcs) = &chat_msg.tool_calls {
        for tc in tcs.iter() {
            if tc.id != tool_call_id {
                continue;
            }

            let args_json = match &tc.arguments {
                serde_json::Value::String(s) => serde_json::from_str::<serde_json::Value>(s)
                    .unwrap_or_else(|_| serde_json::Value::String(s.clone())),
                other => other.clone(),
            };

            match agent_spec.tools.iter().find(|t| t.name() == tc.name) {
                Some(tool) => {
                    tool_result_text =
                        execute_tool_with_context(ctx, session_id, tool.clone(), args_json).await;
                }
                None => {
                    tool_result_text = format!("[ERROR] Tool '{}' not found", tc.name);
                }
            }
            break;
        }
    }

    super::persist_and_notify_tool_result(
        ctx,
        session_id,
        tool_call_id,
        tool_result_text,
        Some(agent_name.to_string()),
        format!(
            "[已批准] tool_call_id={}, tool_name={}",
            tool_call_id, tool_name
        ),
    )
    .await
}

async fn append_rejection_result(
    ctx: &CaelixContext,
    session_id: &str,
    tool_call_id: &str,
    tool_name: &str,
) -> Result<(), ApiError> {
    super::persist_and_notify_tool_result(
        ctx,
        session_id,
        tool_call_id,
        format!("[REJECTED] tool_call_id={} 已被拒绝执行", tool_call_id),
        None,
        format!(
            "[已拒绝] tool_call_id={}, tool_name={}",
            tool_call_id, tool_name
        ),
    )
    .await
}

async fn execute_tool_with_context(
    ctx: &CaelixContext,
    session_id: &str,
    tool: Arc<dyn caelix_api::tool::Tool>,
    args_json: serde_json::Value,
) -> String {
    let session_config = ctx.session_manager.get_session_config(session_id).await;
    let provider_name = session_config
        .as_ref()
        .and_then(|c| c.provider.clone())
        .unwrap_or_default();
    let model_name = session_config
        .as_ref()
        .and_then(|c| c.model.clone())
        .unwrap_or_default();
    let work_dir = std::env::current_dir().unwrap_or_default();

    let cancel_token = ctx
        .agent_run_manager
        .get_cancel_token(session_id)
        .unwrap_or_else(caelix_api::cancel::CancellationToken::new);

    let runtime_ctx = Arc::new(RuntimeContext::new(
        Some(session_id.to_string()),
        None,
        work_dir,
        provider_name,
        model_name,
        ctx.env_config.debug_enabled,
        cancel_token.clone(),
    ));

    let ctx_for_scope = runtime_ctx.clone();
    let tool_fut = async move { tool.execute(args_json).await };

    let cancel_fut = cancel_token.cancelled();
    let timeout_dur = std::time::Duration::from_secs(super::TOOL_EXECUTION_TIMEOUT_SECS);

    tokio::select! {
        result = tool_fut.with_runtime_ctx(ctx_for_scope) => {
            match result {
                caelix_api::tool::ToolResult { error: Some(e), .. } => format!("[ERROR] {}", e),
                caelix_api::tool::ToolResult { output, error: None } => output,
            }
        }
        _ = cancel_fut => {
            format!("[ERROR] Tool execution cancelled")
        }
        _ = tokio::time::sleep(timeout_dur) => {
            format!(
                "[ERROR] Tool execution timed out ({}s)",
                super::TOOL_EXECUTION_TIMEOUT_SECS
            )
        }
    }
}
