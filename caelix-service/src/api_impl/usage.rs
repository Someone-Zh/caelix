//! 用量管理业务逻辑

use caelix_api::error::ApiError;
use caelix_api::provider::GlobalUsageView;
use caelix_api::ContextProvider;
use caelix_runtime::context::CaelixContext;

pub(crate) async fn get_global_usage(ctx: &CaelixContext) -> Result<GlobalUsageView, ApiError> {
    let tracker = ctx
        .usage_tracker()
        .ok_or_else(|| ApiError::InternalError("UsageTracker 未初始化".to_string()))?;
    Ok(tracker.snapshot_global().await)
}
