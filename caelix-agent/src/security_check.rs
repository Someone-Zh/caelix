//! 安全检查模块 — 工具执行前的安全预检查。
//!
//! 从核心工具执行流程中剥离，确保安全检查逻辑不嵌入核心 Agent 循环。
//! 安全检查采用 fail-closed 策略：若 `CaelixContext` 未初始化或检查失败，
//! 返回 `false`（需人工审批），而非阻塞或中断核心流程。

use caelix_api::context::try_caelix_context;
use caelix_api::tool::{ToolApprovalType, ToolPreCheckResult};

/// 检查工具预检结果是否被允许执行。
///
/// 通过全局 `CaelixContext` 获取 `SecurityChecker` 进行路径/URL/命令检查。
/// - 若 `CaelixContext` 未初始化，返回 `false`（fail-closed，需人工审批）
/// - 若安全检查返回 `Err`，返回 `false`（fail-closed）
/// - 若安全检查返回 `Ok`，返回 `true`（允许执行）
pub async fn pre_check_allowed(pre_result: &ToolPreCheckResult) -> bool {
    let Some(ctx) = try_caelix_context() else {
        return false;
    };

    let security_checker = ctx.security_checker();
    match pre_result.approval_type {
        ToolApprovalType::Path => {
            let Some(path) = pre_result
                .parameters
                .get("path")
                .or_else(|| pre_result.parameters.get("file_path"))
                .and_then(|v| v.as_str())
            else {
                return false;
            };
            security_checker.check_path(path).await.is_ok()
        }
        ToolApprovalType::Url => {
            let Some(url) = pre_result.parameters.get("url").and_then(|v| v.as_str()) else {
                return false;
            };
            security_checker.check_url(url).await.is_ok()
        }
        ToolApprovalType::Command => {
            let Some(command) = pre_result
                .parameters
                .get("command")
                .and_then(|v| v.as_str())
            else {
                return false;
            };
            security_checker.check_command(command).await.is_ok()
        }
    }
}
