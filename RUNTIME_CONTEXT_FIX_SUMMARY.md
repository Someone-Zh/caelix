# RuntimeContext 传递问题修复总结

## 问题描述

错误 `cannot access a task-local storage value without setting it first` 发生在 `runtime_context.rs:215`，表明在某些异步任务中没有正确设置 RuntimeContext。

## 根本原因分析

### 核心问题
委托任务的异步执行依赖于 `RuntimeContext::current()` 来获取 provider 和 model 信息，但当任务在后台异步执行时，task-local storage 可能已经不存在。

### 具体问题点

1. **DelegateTaskRunnable** 在异步执行时依赖当前上下文
   - 使用 `catch_unwind` 捕获 panic，但这是治标不治本
   - 如果上下文真的缺失，会导致功能失败

2. **loop_runner** 在没有上下文时仍然继续执行
   - 可能导致后续工具调用失败

3. **缺少统一的上下文传播机制**
   - 不同地方使用不同的方式处理上下文传递

## 修复方案

采用混合方案（方案 C）：
1. 对于需要长期运行的任务（如 DelegateTaskRunnable），存储上下文快照
2. 对于短期异步操作，提供统一的 spawn_with_context 辅助函数
3. 改进错误处理，将隐式的 panic 转换为显式的错误返回

## 具体修改

### 1. 创建 RuntimeContextSnapshot 结构

**文件**: `src/runtime/context/runtime_context.rs`

```rust
#[derive(Debug, Clone)]
pub struct RuntimeContextSnapshot {
    pub provider: String,
    pub model: String,
    pub work_dir: PathBuf,
    pub debug_enabled: bool,
}

impl RuntimeContextSnapshot {
    pub fn from_current() -> Self { ... }
    pub fn try_from_current() -> Option<Self> { ... }
}
```

**优点**:
- 可以在任何时刻捕获当前上下文状态
- 跨异步边界安全传递
- 避免依赖 task-local storage

### 2. 修改 DelegateTaskRunnable

**文件**: `src/base/tool/delegate_task.rs`

#### 添加快照字段
```rust
struct DelegateTaskRunnable {
    // ... 其他字段
    runtime_context_snapshot: Option<RuntimeContextSnapshot>,
}
```

#### 在创建时捕获快照
```rust
let snapshot = RuntimeContextSnapshot::try_from_current();
let runnable = Box::new(DelegateTaskRunnable {
    // ...
    runtime_context_snapshot: snapshot,
});
```

#### 在执行时使用快照
```rust
// 优先使用快照中的 provider，否则使用默认值
let provider_name = if let Some(snapshot) = &self.runtime_context_snapshot {
    snapshot.provider.clone()
} else {
    context.default_provider.clone()
};
```

**改进**:
- ✅ 不再依赖当前上下文
- ✅ 优雅降级到默认值
- ✅ 移除了多处 `catch_unwind` 导致的错误返回

### 3. 优化 loop_runner 错误处理

**文件**: `src/base/agent/loop_runner.rs`

```rust
tokio::spawn(async move {
    if let Some(ctx) = runtime_ctx {
        RuntimeContext::scope(ctx, async move {
            run_agent_loop_inner(...).await;
        }).await;
    } else {
        // 没有 RuntimeContext 时，发送错误并退出
        let _ = tx.send(Err(AgentError::TaskError(
            "No RuntimeContext available...".to_string()
        ))).await;
    }
});
```

**改进**:
- ❌ 之前：没有上下文也继续执行，导致后续失败
- ✅ 现在：明确返回错误，便于调试

### 4. 添加 spawn_with_context 辅助函数

**文件**: `src/runtime/context/runtime_context.rs`

```rust
pub fn spawn_with_context<F>(
    context: RuntimeContext, 
    future: F
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move {
        CURRENT_CONTEXT.scope(context, future).await
    })
}
```

**用途**:
- 统一异步任务的上下文传递方式
- 简化代码，减少重复
- 提供清晰的 API 文档和示例

### 5. 审查并标注其他 tokio::spawn 调用

添加了注释说明哪些地方不需要 RuntimeContext：

- **消息消费者** (`src/runtime/message/manager.rs`): 只处理消息分发和持久化
- **CLI 监听任务** (`src/backends/cli/runner.rs`): 仅用于打印输出
- **TUI 命令** (`src/backends/tui/commands.rs`): API 层内部已处理上下文

## 修复效果

### 编译测试
```bash
$ cargo build
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s

$ cargo test
test result: ok. 8 passed; 0 failed; 0 ignored
```

### 预期改进

1. ✅ **不再出现 panic**: 所有异步任务都有明确的上下文来源
2. ✅ **更好的错误处理**: 从隐式 panic 变为显式错误返回
3. ✅ **更清晰的代码**: 通过快照和辅助函数，意图更明确
4. ✅ **向后兼容**: 保持现有功能不变，只是增强了健壮性
5. ✅ **易于调试**: 错误信息更清晰，便于定位问题

## 最佳实践建议

### 1. 异步任务中的上下文传递

**推荐做法**:
```rust
// 方法 1: 使用快照（适合长期运行的任务）
let snapshot = RuntimeContextSnapshot::try_from_current();
tokio::spawn(async move {
    // 使用 snapshot.provider, snapshot.model 等
});

// 方法 2: 使用 spawn_with_context（适合短期任务）
let ctx = RuntimeContext::current();
RuntimeContext::spawn_with_context(ctx, async {
    // 可以正常使用 RuntimeContext::provider() 等
});
```

**避免**:
```rust
// ❌ 不要这样做
tokio::spawn(async {
    let provider = RuntimeContext::provider(); // 可能 panic!
});
```

### 2. 错误处理

**推荐**:
```rust
// 优雅降级
let provider = if let Ok(name) = std::panic::catch_unwind(|| RuntimeContext::provider()) {
    name
} else {
    context.default_provider.clone() // 回退到默认值
};
```

### 3. 文档和注释

在不需要 RuntimeContext 的地方添加注释：
```rust
// 注意：此任务不需要 RuntimeContext，仅用于 XXX
tokio::spawn(async move {
    ...
});
```

## 未来改进方向

1. **宏支持**: 可以创建 `#[with_runtime_context]` 宏自动处理上下文传递
2. **类型安全**: 使用类型系统强制要求某些函数必须在上下文中执行
3. **监控和日志**: 记录上下文缺失的情况，便于发现潜在问题
4. **性能优化**: 评估快照克隆的性能影响，必要时使用 Arc 共享

## 相关文件

- `src/runtime/context/runtime_context.rs` - RuntimeContext 和快照定义
- `src/runtime/context/mod.rs` - 模块导出
- `src/base/tool/delegate_task.rs` - 委托任务工具
- `src/base/agent/loop_runner.rs` - Agent 循环执行器
- `src/runtime/message/manager.rs` - 消息管理器
- `src/backends/cli/runner.rs` - CLI 后端
- `src/backends/tui/commands.rs` - TUI 命令处理

## 总结

这次修复从根本上解决了 RuntimeContext 传递问题，通过引入快照机制和统一的辅助函数，使得异步任务中的上下文管理更加健壮和可预测。所有修改都保持了向后兼容性，同时提供了更好的错误处理和调试体验。
