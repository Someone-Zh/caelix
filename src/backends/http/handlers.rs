use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::StreamExt;
use std::sync::Arc;

use crate::api::{CaelixApi, CaelixApiImpl};
use crate::api::types::{
    ChatRequest, CreateSessionResponse, DefaultConfigResponse, AgentListResponse
};

pub type ApiState = Arc<CaelixApiImpl>;

/// 获取默认配置
pub async fn get_default_config(
    State(api): State<ApiState>,
) -> Json<DefaultConfigResponse> {
    Json(DefaultConfigResponse {
        default_provider: api.get_default_provider(),
        default_model: api.get_default_model(),
    })
}

/// 创建新会话
pub async fn create_session(
    State(api): State<ApiState>,
) -> Json<CreateSessionResponse> {
    let session_id = api.create_session();
    Json(CreateSessionResponse { session_id })
}

/// 设置会话提供者
pub async fn set_session_provider(
    State(api): State<ApiState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let provider = payload.get("provider")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    api.set_session_provider(&session_id, provider)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    Ok(StatusCode::OK)
}

/// 设置会话模型
pub async fn set_session_model(
    State(api): State<ApiState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let model = payload.get("model")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    api.set_session_model(&session_id, model)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    Ok(StatusCode::OK)
}

/// 获取所有 agent 列表
pub async fn list_agents(
    State(api): State<ApiState>,
) -> Json<AgentListResponse> {
    let agents = api.list_agents().await;
    Json(AgentListResponse { agents })
}

/// 流式聊天（SSE）
pub async fn chat_stream(
    State(api): State<ApiState>,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>, StatusCode> {
    let stream = api.chat_stream(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let event_stream = stream.map(|chunk_result| {
        let event = match chunk_result {
            Ok(chunk) => {
                let json = serde_json::to_string(&chunk).unwrap_or_default();
                Event::default().data(json)
            }
            Err(e) => {
                let error_json = serde_json::json!({ "error": e.to_string() });
                Event::default().data(error_json.to_string())
            }
        };
        Ok(event)
    });

    Ok(Sse::new(event_stream))
}
