pub mod hooks;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::base::agent::AgentSpec;
use crate::enhancement::hooks::AgentHook;

/// 钩子注册中心
/// 管理所有Agent增强钩子,并在Agent执行前应用它们
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

    /// 应用所有钩子到AgentSpec
    pub async fn apply_hooks(&self, agent_spec: &mut AgentSpec) {
        let hooks = self.hooks.read().await;
        for hook in hooks.iter() {
            println!("Applying hook: {}", hook.name());
            hook.enhance_agent(agent_spec);
        }
    }

    /// 获取已注册的钩子数量
    pub async fn hook_count(&self) -> usize {
        let hooks = self.hooks.read().await;
        hooks.len()
    }
}