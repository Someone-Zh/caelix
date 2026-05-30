# RuntimeContext Trait Combinator Implementation Plan

## 1. 任务概述

在 `runtime_context.rs` 中实现更符合 Rust 惯用法的 trait combinator 模式，为 Future 类型添加 `.with_current_ctx()` 方法，自动传播上下文，减少外部代码耦合，提高代码可读性。

## 2. 当前状态分析

### 2.1 现有实现
- **存储机制**: 使用 `tokio::task_local!` 存储 `RuntimeContext`
- **访问方式**: 通过 `RuntimeContext::current()`, `RuntimeContext::session_id()` 等静态方法访问
- **作用域管理**: `RuntimeContext::scope(context, future)` 建立上下文边界
- **任务spawn**: `RuntimeContext::spawn_with_context()` 在新任务中传递上下文

### 2.2 现有使用模式

**模式1: 显式上下文创建和传递**
```rust
let runtime_ctx = RuntimeContext::new(/* ... */);
RuntimeContext::scope(runtime_ctx, async move {
    // 业务逻辑
}).await;
```

**模式2: 安全地捕获当前上下文**
```rust
let runtime_ctx = std::panic::catch_unwind(RuntimeContext::current).ok();
if let Some(ctx) = runtime_ctx {
    RuntimeContext::scope(ctx, async move {
        // 业务逻辑
    }).await;
}
```

**模式3: 在嵌套spawn中使用**
```rust
tokio::spawn(async move {
    RuntimeContext::spawn_with_context(ctx, async {
        // 业务逻辑
    }).await
});
```

### 2.3 发现的问题

1. **样板代码重复**: 每次都需要 `catch_unwind` + `if let Some` 检查
2. **显式上下文传递**: 调用者必须显式管理 context 的生命周期
3. **耦合度高**: 业务逻辑与上下文管理逻辑混杂
4. **错误处理分散**: 不同的使用位置有不同的错误处理策略

## 3. 设计方案

### 3.1 核心设计

创建一个 trait combinator `FutureContextExt`，为所有 `Future` 类型提供 `.with_current_ctx()` 方法：

```rust
pub trait FutureContextExt {
    fn with_current_ctx(self) -> WithCurrentContextFuture<Self>
    where
        Self: Sized;
}
```

### 3.2 实现策略

**策略1: 捕获-传播模式**
- 在调用点捕获当前上下文（如果存在）
- 自动在新作用域中执行 Future
- 如果没有上下文，直接执行 Future

**策略2: 类型安全的错误处理**
- 提供 `Result` 版本的方法返回错误
- 提供 `Option` 版本的方法优雅降级
- 提供 `unwrap` 版本的方法在失败时 panic

**策略3: 向后兼容**
- 保留所有现有的 API
- 新方法作为便捷包装器
- 逐步迁移现有代码

### 3.3 API 设计

```rust
// 主方法：自动传播当前上下文（如果存在）
fn with_current_ctx(self) -> WithCurrentContextFuture<Self>
where Self: Sized;

// 变体1：要求必须存在上下文，不存在则 panic
fn with_ctx(self) -> WithContextFuture<Self>
where Self: Sized;

// 变体2：返回 Result，可自定义错误处理
fn with_ctx_result<E>(self) -> WithContextResultFuture<Self, E>
where Self: Sized;

// 变体3：spawn 版本，自动在新任务中传播上下文
fn spawn_with_current_ctx(self) -> JoinHandle<Self::Output>
where Self: Sized + Send + 'static,
      Self::Output: Send + 'static;
```

## 4. 实现步骤

### 步骤1: 创建新的 trait 文件

**文件**: `runtime_context_ext.rs`

**内容**:
```rust
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::JoinHandle;

/// Future wrapper that automatically propagates RuntimeContext
pub struct WithCurrentContextFuture<F> {
    future: F,
    captured_context: Option<crate::RuntimeContext>,
}

impl<F> WithCurrentContextFuture<F> {
    pub fn new(future: F, context: Option<crate::RuntimeContext>) -> Self {
        Self {
            future,
            captured_context: context,
        }
    }
}

impl<F> Future for WithCurrentContextFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 实现将在步骤2详细说明
    }
}
```

### 步骤2: 实现 Future trait

关键实现要点：

1. **捕获上下文**: 在创建时使用 `RuntimeContextSnapshot::try_from_current()`
2. **作用域执行**: 使用 `RuntimeContext::scope()` 执行内部 Future
3. **类型擦除**: 正确处理 `Pin<Box<dyn Future>>` 等情况
4. **错误传播**: 保留所有内部 Future 的错误类型

### 步骤3: 实现 trait combinator

```rust
pub trait FutureContextExt: Sized {
    fn with_current_ctx(self) -> WithCurrentContextFuture<Self>;
}
```

### 步骤4: 实现 spawn 扩展

```rust
impl FutureContextExt for dyn Future<Output = T> + Send + 'static {
    fn with_current_ctx(self) -> WithCurrentContextFuture<Self> {
        // 实现
    }
}
```

## 5. 重构现有代码

### 5.1 api_impl.rs (2处)

**位置1: 行234**
```rust
// Before:
caelix_runtime::context::RuntimeContext::scope(runtime_ctx, async move {
    // 业务逻辑
}).await;

// After:
runtime_ctx.with_current_ctx(async move {
    // 业务逻辑
}).await
```

**位置2: 行495**
```rust
// Before:
let result = caelix_runtime::context::RuntimeContext::scope(runtime_ctx, async move {
    // 业务逻辑
}).await;

// After:
let result = runtime_ctx.with_current_ctx(async move {
    // 业务逻辑
}).await;
```

### 5.2 loop_runner.rs (1处)

**位置: 行47**
```rust
// Before:
if let Some(ctx) = runtime_ctx {
    caelix_runtime::context::RuntimeContext::scope(ctx, async move {
        run_agent_loop_inner(agent, messages, llm_provider, config, tx).await;
    }).await;
}

// After:
runtime_ctx.with_current_ctx(async move {
    run_agent_loop_inner(agent, messages, llm_provider, config, tx).await;
}).await;
```

### 5.3 manager.rs (1处)

**位置: 行355**
```rust
// Before:
let result = RuntimeContext::scope(runtime_ctx, async {
    runnable.run().await
}).await;

// After:
let result = runtime_ctx.with_current_ctx(async {
    runnable.run().await
}).await;
```

### 5.4 delegate_task.rs (多处)

**关键重构**: 消除所有 `std::panic::catch_unwind` 调用

现有代码模式:
```rust
let provider_name = match std::panic::catch_unwind(|| RuntimeContext::provider()) {
    Ok(name) => name,
    Err(_) => context.default_provider.clone(),
};
```

建议改为:
```rust
// 使用上下文感知的 provider 获取
let provider_name = runtime_ctx
    .provider()
    .unwrap_or_else(|| context.default_provider.clone());
```

**注意**: delegate_task.rs 中有多处使用 `catch_unwind`，这些主要用于在工具执行时获取上下文信息，需要评估哪些可以迁移到新的模式。

### 5.5 message_bus_hook.rs (多处)

**位置: 行51-67**
```rust
// Before:
if let Ok(runtime_ctx) = std::panic::catch_unwind(|| {
    crate::context::RuntimeContext::current()
}) {
    if let Some(context_provider) = runtime_ctx.get_context_provider() {
        // 业务逻辑
    }
}

// After:
if let Some(runtime_ctx) = RuntimeContext::try_current() {
    if let Some(context_provider) = runtime_ctx.get_context_provider() {
        // 业务逻辑
    }
}
```

## 6. 辅助改进

### 6.1 添加 try_current() 方法

在 `RuntimeContext` 中添加安全的上下文获取方法：

```rust
impl RuntimeContext {
    /// 尝试获取当前上下文（安全版本）
    ///
    /// # Returns
    /// 如果在有效的 RuntimeContext 中，返回 Some(ctx)
    /// 否则返回 None
    pub fn try_current() -> Option<RuntimeContext> {
        std::panic::catch_unwind(Self::current).ok()
    }

    /// 获取当前 Provider，失败时返回默认值
    pub fn try_provider() -> Option<String> {
        Self::try_current().map(|ctx| ctx.provider.clone())
    }

    // 类似的方法...
}
```

### 6.2 添加实例方法的 getter

添加基于实例的便捷方法：

```rust
impl RuntimeContext {
    /// 实例版本：获取 Provider
    pub fn provider(&self) -> String {
        self.provider.clone()
    }

    /// 实例版本：获取 Model
    pub fn model(&self) -> String {
        self.model.clone()
    }

    // 类似的方法...
}
```

## 7. 扩展 Future 支持

### 7.1 为 Box<dyn Future> 提供支持

```rust
impl<T: ?Sized> FutureContextExt for T
where
    T: Future,
{
    fn with_current_ctx(self) -> WithCurrentContextFuture<Self> {
        let context = RuntimeContextSnapshot::try_from_current();
        WithCurrentContextFuture::new(self, context)
    }
}
```

### 7.2 为 Pin<Box<dyn Future>> 提供支持

```rust
impl<T: ?Sized> FutureContextExt for Pin<Box<T>>
where
    T: Future,
{
    fn with_current_ctx(self) -> WithCurrentContextFuture<Pin<Box<T>>> {
        let context = RuntimeContextSnapshot::try_from_current();
        WithCurrentContextFuture::new(self, context)
    }
}
```

## 8. 测试计划

### 8.1 单元测试

1. **基础功能测试**: 验证 `.with_current_ctx()` 正确传播上下文
2. **空上下文测试**: 验证在没有上下文时优雅降级
3. **嵌套调用测试**: 验证多层嵌套时的上下文一致性
4. **spawn 集成测试**: 验证在新任务中正确传播上下文
5. **错误传播测试**: 验证内部 Future 的错误正确传播

### 8.2 集成测试

1. **完整流程测试**: 模拟完整的 agent 执行流程
2. **并发场景测试**: 验证多任务并发时的上下文隔离
3. **性能测试**: 验证新实现没有显著性能开销

### 8.3 回归测试

1. **现有功能回归**: 确保所有现有 `scope()` 调用仍然正常工作
2. **向后兼容**: 确保现有代码无需修改即可编译运行

## 9. 文档更新

### 9.1 代码注释

- 为新 API 添加详细的文档注释
- 包含使用示例
- 说明错误处理策略

### 9.2 使用指南更新

更新 `docs/RUNTIME_CONTEXT_USAGE.md`:
- 添加新 API 的使用示例
- 对比新旧两种写法
- 说明迁移策略

## 10. 风险评估和缓解

### 10.1 风险

1. **引入新的错误处理路径**: 可能影响现有错误处理逻辑
2. **性能开销**: 上下文捕获和作用域切换可能引入延迟
3. **与现有模式的冲突**: 两种模式并存可能造成困惑

### 10.2 缓解措施

1. **渐进式迁移**: 保留所有旧 API，允许逐步迁移
2. **性能测试**: 在重构前建立性能基准，重构后验证
3. **清晰的文档**: 明确说明每种模式的适用场景
4. **严格测试**: 覆盖所有边缘情况

## 11. 实施时间表

1. **阶段1** (步骤1-3): 核心实现 - 1-2小时
2. **阶段2** (步骤4): API 重构 - 2-3小时
3. **阶段3** (步骤5): 工具类重构 - 1-2小时
4. **阶段4** (步骤6-7): 测试和文档 - 2-3小时

总计: 约 8-10 小时

## 12. 验收标准

1. ✅ 所有现有测试通过
2. ✅ 新 API 提供与 `scope()` 等效的功能
3. ✅ 代码可读性显著提升（减少样板代码）
4. ✅ 没有引入新的编译警告
5. ✅ 性能测试无显著退化
6. ✅ 文档完整且准确
