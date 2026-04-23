use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

use crate::api::CaelixApiImpl;
use super::handlers::*;

/// 启动 HTTP 服务器
pub async fn start_http_server(api: Arc<CaelixApiImpl>, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(api);
    
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("🚀 HTTP Server starting on http://{}", addr);
    
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service()
    ).await?;
    
    Ok(())
}

fn create_router(api: Arc<CaelixApiImpl>) -> Router {
    Router::new()
        // 默认配置
        .route("/api/providers/default", get(get_default_config))
        .route("/api/models/default", get(get_default_config))
        // 会话管理
        .route("/api/sessions", post(create_session))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{session_id}/provider", put(set_session_provider))
        .route("/api/sessions/{session_id}/model", put(set_session_model))
        // 会话消息历史
        .route("/api/sessions/{session_id}/messages", get(get_session_messages))
        // 会话通知历史
        .route("/api/sessions/{session_id}/notifications", get(get_session_notifications))
        // Agent 列表
        .route("/api/agents", get(list_agents))
        // 任务管理
        .route("/api/tasks", get(list_tasks))
        // 提供者管理
        .route("/api/providers", get(get_providers))
        .route("/api/providers/{name}/models", get(get_provider_models))
        // 流式聊天
        .route("/api/chat/stream", post(chat_stream))
        // 添加 CORS 支持
        .layer(CorsLayer::permissive())
        .with_state(api)
}
