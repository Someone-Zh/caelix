use std::sync::Arc;
use tokio_util::sync::CancellationToken as TokioCancellationToken;

/// 基于 `tokio_util::sync::CancellationToken` 的取消令牌
///
/// 使用 `tokio_util` 生产级实现，`cancelled()` 返回的 future 是 zero-allocation 的
/// `WaitForCancellationFuture`，可在 `select!` 中通过 `&mut` 复用。
///
/// 与 `Notify` 方案的关键区别：
/// - `cancelled()` 在 `cancel()` 后立即返回，不会丢失信号 — 即使 `cancel()` 发生在
///   订阅者注册之前，后续的 `cancelled()` 也会立即返回。
/// - 相比 `watch::<bool>` 方案，`CancellationToken` 是原子级操作，不依赖管道状态，
///   所以 `Drop` 后不再触发；但我们的 wrapper 通过 `Arc` 引用计数共享。
#[derive(Clone)]
pub struct CancellationToken {
    inner: Arc<TokioCancellationToken>,
}

impl CancellationToken {
    /// 创建一个未取消的令牌
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TokioCancellationToken::new()),
        }
    }

    /// 触发取消
    ///
    /// 所有通过 `cancelled()` 等待的 future 都会立即返回。
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// 等待取消信号
    ///
    /// 返回一个可在 `select!` 中复用的 future。若已取消则立即返回。
    /// 基于 `tokio_util` 的零分配实现，适合高频轮询。
    pub async fn cancelled(&self) {
        self.inner.cancelled().await;
    }

    /// 派生子令牌。
    ///
    /// 父令牌取消会级联到子令牌；子令牌取消不会反向取消父令牌。
    pub fn child_token(&self) -> Self {
        Self {
            inner: Arc::new(self.inner.child_token()),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
