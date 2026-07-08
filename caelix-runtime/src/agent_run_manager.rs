use caelix_api::cancel::CancellationToken;
use caelix_api::context::AgentRunManagerTrait;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::task::{AbortHandle, JoinHandle};

/// 单次 Agent 运行的注册信息
struct AgentRunInfo {
    /// 唯一标识本次运行，用于区分同一 session 的新旧任务，
    /// 唯一标识本次运行，用于区分同一 session 的新旧任务，
    /// 避免旧任务的 guard 误删新任务在 map 中的 entry。
    run_id: u64,
    cancel_token: CancellationToken,
    /// spawn 后回填。在 `register` 与 `set_join_handle` 之间的窗口期可能为 None，
    /// 此时 `stop_agent` 仅靠 cancel_token 通知（任务 spawn 后会在首个检查点退出）。
    abort_handle: Option<AbortHandle>,
    /// 用于优雅等待 Agent 自然退出。
    join_handle: Option<JoinHandle<()>>,
}

/// Agent 运行管理器
///
/// 维护 session_id → 当前运行 Agent 的映射，支持紧急停止。
/// 采用分级取消策略：先 `cancel()` 触发优雅退出（产出 `Stopped` chunk），
/// 超时后再 `abort()` 强制中止。
pub struct AgentRunManager {
    runs: DashMap<String, AgentRunInfo>,
    next_run_id: AtomicU64,
}

/// 优雅退出的最长等待时长（Agent 有机会产出 Stopped chunk）。
/// LLM 流式响应可能需要 1-2s 才出下一个 chunk，500ms 可能先于 Stopped 完成而强制 abort。
const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(2);

impl AgentRunManager {
    pub fn new() -> Self {
        Self {
            runs: DashMap::new(),
            next_run_id: AtomicU64::new(1),
        }
    }

    /// 预注册一次 Agent 运行（在 spawn 之前调用）
    ///
    /// 返回 `run_id`，用于后续 `set_join_handle` 与 `RunGuard` 中比对。
    /// 若同一 session 已有运行中的 Agent，仅触发其 cancel_token（不 abort），
    /// 让旧 Agent 有机会产出 `Stopped` chunk 后自然退出。
    pub fn register(&self, session_id: String, cancel_token: CancellationToken) -> u64 {
        let run_id = self.next_run_id.fetch_add(1, Ordering::Relaxed);
        let info = AgentRunInfo {
            run_id,
            cancel_token,
            abort_handle: None,
            join_handle: None,
        };
        if let Some(old) = self.runs.insert(session_id, info) {
            old.cancel_token.cancel();
            // 旧任务若已 set abort_handle，则保留它供 stop 时强制中止；
            // 但这里我们不再持有 old 的引用——insert 已把旧值返回。
            // 旧 Agent 的 RunGuard 会因 run_id 不匹配而 no-op。
        }
        run_id
    }

    /// spawn 后回填 JoinHandle 与 AbortHandle
    ///
    /// 若 entry 已被 `stop_agent` 移除或被新 run 覆盖（run_id 不匹配），
    /// 则丢弃 handle（对应任务会在 cancel 信号下自行退出）。
    pub fn set_handles(&self, session_id: &str, run_id: u64, join_handle: JoinHandle<()>) {
        if let Some(mut entry) = self.runs.get_mut(session_id) {
            if entry.run_id == run_id {
                entry.abort_handle = Some(join_handle.abort_handle());
                entry.join_handle = Some(join_handle);
            }
        }
    }

    /// 注销指定 run_id 的运行
    ///
    /// 仅当 map 中当前 entry 的 run_id 与传入值一致时才移除，
    /// 避免旧任务的 guard 误删已被新任务占据的 entry。
    pub fn unregister(&self, session_id: &str, run_id: u64) {
        self.runs
            .remove_if(session_id, |_, info| info.run_id == run_id);
    }

    /// 紧急停止：分级取消
    ///
    /// 1. 移除 entry 并触发 cancel_token（让 Agent 在检查点产出 `Stopped`）
    /// 2. 若 join_handle 已就绪，等待最多 `GRACEFUL_STOP_TIMEOUT` 让其自然退出
    /// 3. 超时则 `abort()` 强制中止
    ///
    /// 返回 `true` 表示找到并触发了停止，`false` 表示该 session 当前无运行中的 Agent。
    async fn stop(&self, session_id: &str) -> bool {
        let info = match self.runs.remove(session_id) {
            Some((_, i)) => i,
            None => return false,
        };
        info.cancel_token.cancel();

        if let Some(handle) = info.join_handle {
            match tokio::time::timeout(GRACEFUL_STOP_TIMEOUT, handle).await {
                Ok(Ok(())) => {
                    tracing::info!(
                        session_id = session_id,
                        "agent stopped gracefully within grace period"
                    );
                }
                Ok(Err(e)) if e.is_cancelled() => {
                    tracing::info!(
                        session_id = session_id,
                        "agent task was cancelled (after external abort)"
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        session_id = session_id,
                        error = %e,
                        is_panic = e.is_panic(),
                        "agent task ended with join error"
                    );
                }
                Err(_timeout) => {
                    tracing::warn!(
                        session_id = session_id,
                        grace_ms = GRACEFUL_STOP_TIMEOUT.as_millis() as u64,
                        "agent did not stop in time, force-aborting"
                    );
                    if let Some(abort) = info.abort_handle {
                        abort.abort();
                    }
                }
            }
        } else {
            tracing::info!(
                session_id = session_id,
                "stop requested before join_handle set; cancel signal will be honored on spawn"
            );
        }
        true
    }
}

impl Default for AgentRunManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentRunManagerTrait for AgentRunManager {
    async fn stop_agent(&self, session_id: &str) -> bool {
        self.stop(session_id).await
    }
}

/// RAII 守卫：spawn 的任务结束时（无论正常返回、panic 还是被 abort）
/// 自动从 `AgentRunManager` 中注销对应 entry。
///
/// 仅当 map 中当前 entry 的 `run_id` 与本 guard 持有的 `run_id` 一致时才移除，
/// 避免误删已被新 run 覆盖的 entry。
pub struct RunGuard {
    arm: Arc<AgentRunManager>,
    session_id: String,
    run_id: u64,
}

impl RunGuard {
    pub fn new(arm: Arc<AgentRunManager>, session_id: String, run_id: u64) -> Self {
        Self {
            arm,
            session_id,
            run_id,
        }
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.arm.unregister(&self.session_id, self.run_id);
    }
}
