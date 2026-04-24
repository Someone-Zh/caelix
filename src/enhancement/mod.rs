pub mod hooks;
pub mod commands;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::enhancement::hooks::{AgentHook, InitContext, PreContext, PostContext, ErrorContext};

/// 钩子注册中心
/// 管理所有Agent增强钩子，并在Agent生命周期的不同阶段应用它们
#[derive(Clone)]
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistry")
            .field("hook_count", &self.hooks.try_read().map(|h| h.len()).unwrap_or(0))
            .finish()
    }
}

impl HookRegistry {
    /// 创建新的钩子注册中心
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册钩子
    pub async fn register_hook(&self, hook: Arc<dyn AgentHook>) {
        let mut hooks = self.hooks.write().await;
        println!("Registering hook: {}", hook.name());
        hooks.push(hook);
    }

    /// 执行Init阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_init(&self, ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                println!("Executing init hook: {}", hook.name());
                hook.on_init(ctx).await?;
            }
        }
        Ok(())
    }

    /// 执行Pre阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_pre(&self, ctx: &mut PreContext) -> Result<(), anyhow::Error> {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                println!("Executing pre-process hook: {}", hook.name());
                hook.on_pre_process(ctx).await?;
            }
        }
        Ok(())
    }

    /// 执行Post阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_post(&self, ctx: &PostContext) -> Result<(), anyhow::Error> {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                println!("Executing post-process hook: {}", hook.name());
                hook.on_post_process(ctx).await?;
            }
        }
        Ok(())
    }

    /// 执行Error阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_error(&self, ctx: &ErrorContext) -> Result<(), anyhow::Error> {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                println!("Executing error hook: {}", hook.name());
                // Error钩子失败不中断，只记录日志
                if let Err(e) = hook.on_error(ctx).await {
                    eprintln!("Error hook {} failed: {:?}", hook.name(), e);
                }
            }
        }
        Ok(())
    }

    /// 获取已注册的钩子数量
    pub async fn hook_count(&self) -> usize {
        let hooks = self.hooks.read().await;
        hooks.len()
    }
}