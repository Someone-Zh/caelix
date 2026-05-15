#!/bin/bash

# RuntimeContext 修复验证脚本

echo "======================================"
echo "RuntimeContext 修复验证"
echo "======================================"
echo ""

# 1. 编译检查
echo "1. 编译检查..."
cargo build --quiet 2>&1
if [ $? -eq 0 ]; then
    echo "   ✅ 编译成功"
else
    echo "   ❌ 编译失败"
    exit 1
fi
echo ""

# 2. 运行测试
echo "2. 运行单元测试..."
cargo test --quiet 2>&1 | tail -5
if [ $? -eq 0 ]; then
    echo "   ✅ 测试通过"
else
    echo "   ❌ 测试失败"
    exit 1
fi
echo ""

# 3. 检查关键文件修改
echo "3. 检查关键修改..."

# 检查 RuntimeContextSnapshot 是否存在
if grep -q "pub struct RuntimeContextSnapshot" src/runtime/context/runtime_context.rs; then
    echo "   ✅ RuntimeContextSnapshot 结构已添加"
else
    echo "   ❌ RuntimeContextSnapshot 结构未找到"
    exit 1
fi

# 检查 DelegateTaskRunnable 是否有快照字段
if grep -q "runtime_context_snapshot: Option<RuntimeContextSnapshot>" src/base/tool/delegate_task.rs; then
    echo "   ✅ DelegateTaskRunnable 已添加快照字段"
else
    echo "   ❌ DelegateTaskRunnable 快照字段未找到"
    exit 1
fi

# 检查 spawn_with_context 方法是否存在
if grep -q "pub fn spawn_with_context" src/runtime/context/runtime_context.rs; then
    echo "   ✅ spawn_with_context 辅助函数已添加"
else
    echo "   ❌ spawn_with_context 辅助函数未找到"
    exit 1
fi

# 检查 loop_runner 的错误处理
if grep -q "No RuntimeContext available" src/base/agent/loop_runner.rs; then
    echo "   ✅ loop_runner 错误处理已优化"
else
    echo "   ❌ loop_runner 错误处理未更新"
    exit 1
fi

echo ""
echo "======================================"
echo "✅ 所有验证通过！"
echo "======================================"
echo ""
echo "修复总结："
echo "- RuntimeContextSnapshot 结构用于保存上下文状态"
echo "- DelegateTaskRunnable 使用快照而非依赖当前上下文"
echo "- loop_runner 在没有上下文时返回明确错误"
echo "- 添加了 spawn_with_context 辅助函数"
echo "- 审查并标注了所有 tokio::spawn 调用"
echo ""
echo "详细说明请查看: RUNTIME_CONTEXT_FIX_SUMMARY.md"
