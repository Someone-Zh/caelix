//! 任务管理业务逻辑

use super::validate_session_id;
use caelix_api::error::ApiError;
use caelix_api::task::TaskMeta;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn list_tasks(
    ctx: &CaelixContext,
    session_id: Option<&str>,
) -> Result<Vec<TaskMeta>, ApiError> {
    if let Some(sid) = session_id {
        validate_session_id(sid)?;
    }

    let task_manager = match &ctx.task_manager {
        Some(tm) => tm,
        None => {
            return Err(ApiError::InternalError(
                "TaskManager not initialized".to_string(),
            ));
        }
    };

    Ok(task_manager.list_tasks(session_id).await)
}
