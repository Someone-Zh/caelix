//! 通知管理业务逻辑

use super::validate_session_id;
use caelix_api::error::ApiError;
use caelix_api::message::NotificationMessage;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn get_session_notifications(
    _ctx: &CaelixContext,
    session_id: &str,
) -> Result<Vec<NotificationMessage>, ApiError> {
    validate_session_id(session_id)?;

    Err(ApiError::InternalError(
        "通知消息不再持久化，请通过 subscribe_chat_stream 订阅实时通知".to_string(),
    ))
}
