pub mod skill_hook;
pub mod loader;
pub mod message_bus_hook;
pub mod tool_result_check_hook;

// 从 caelix-api 导入已迁移的类型
pub use caelix_api::hooks::*;

use tokio::sync::RwLock;
use std::sync::Arc;
use caelix_api::agent::AgentSpec;
use caelix_api::provider::ChatMessage;
use caelix_api::tool::ToolResult;
use caelix_api::agent::AgentOutputChunk;

/// 钩子注册中心
/// 管理所有Agent增强钩子，并在Agent生命周期的不同阶段应用它们
#[derive(Clone)]
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    // 按能力预分类的钩子列表
    init_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    pre_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    post_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    error_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    pre_tool_exec_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    post_tool_exec_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
    message_update_hooks: Arc<RwLock<Vec<Arc<dyn AgentHook>>>>,
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
            init_hooks: Arc::new(RwLock::new(Vec::new())),
            pre_hooks: Arc::new(RwLock::new(Vec::new())),
            post_hooks: Arc::new(RwLock::new(Vec::new())),
            error_hooks: Arc::new(RwLock::new(Vec::new())),
            pre_tool_exec_hooks: Arc::new(RwLock::new(Vec::new())),
            post_tool_exec_hooks: Arc::new(RwLock::new(Vec::new())),
            message_update_hooks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册钩子
    pub async fn register_hook(&self, hook: Arc<dyn AgentHook>) {
        let mut hooks = self.hooks.write().await;
        let caps = hook.capabilities();
        
        println!("Registering hook: {} with capabilities: {:?}", hook.name(), caps);
        
        // 添加到总列表
        hooks.push(hook.clone());
        
        // 根据能力分类存储
        if caps.contains(HookCapability::INIT) {
            self.init_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::PRE_PROCESS) {
            self.pre_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::POST_PROCESS) {
            self.post_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::ERROR) {
            self.error_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::PRE_TOOL_EXEC) {
            self.pre_tool_exec_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::POST_TOOL_EXEC) {
            self.post_tool_exec_hooks.write().await.push(hook.clone());
        }
        if caps.contains(HookCapability::ON_MESSAGE_UPDATE) {
            self.message_update_hooks.write().await.push(hook.clone());
        }
    }

    /// 获取已注册的钩子数量
    pub async fn hook_count(&self) -> usize {
        let hooks = self.hooks.read().await;
        hooks.len()
    }

    /// 应用Init阶段钩子到AgentSpec（用于Agent注册时的一次性增强）
    /// 
    /// # Arguments
    /// * `agent_spec` - 要增强的AgentSpec可变引用
    /// * `session_id` - 会话ID（可选，用于日志）
    /// 
    /// 这个方法应该在Agent注册时调用，确保每个Agent只被增强一次
    pub async fn apply_init_hooks(
        &self,
        agent_spec: &mut AgentSpec,
        session_id: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        // 直接使用预分类的 init_hooks，而不是遍历所有 hooks
        let hooks = self.init_hooks.read().await;
        let session_id = session_id.unwrap_or("init").to_string();
        
        for hook in hooks.iter() {
            if hook.should_apply(&agent_spec.name, agent_spec.group.as_ref().map(|s| s.as_str())) {
                println!("Applying init hook '{}' to agent '{}'", hook.name(), agent_spec.name);
                
                // 创建BaseContext
                let base_ctx = BaseContext {
                    session_id: session_id.clone(),
                    request_id: format!("{}-init", session_id),
                    span_id: format!("{}-init", session_id),
                    agent_name: agent_spec.name.clone(),
                    agent_group: agent_spec.group.as_ref().map(|g| g.to_string()),
                };
                
                // 创建InitContext
                let mut init_ctx = InitContext {
                    base: base_ctx,
                    agent_spec,
                };
                
                // 执行钩子
                hook.on_init(&mut init_ctx).await?;
            }
        }
        
        Ok(())
    }
    
    /// 执行消息更新钩子
    pub async fn execute_message_update(
        &self,
        ctx: &MessageUpdateContext,
    ) -> Result<(), anyhow::Error> {
        let hooks = self.message_update_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                hook.on_message_update(ctx).await?;
            }
        }
        Ok(())
    }
    
    /// 执行工具执行前钩子
    pub async fn execute_pre_tool_exec(
        &self,
        ctx: &mut PreToolExecContext,
    ) -> Result<(), anyhow::Error> {
        let hooks = self.pre_tool_exec_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                hook.on_pre_tool_exec(ctx).await?;
            }
        }
        Ok(())
    }
    
    /// 执行工具执行后钩子
    pub async fn execute_post_tool_exec(
        &self,
        ctx: &mut PostToolExecContext,
    ) -> Result<(), anyhow::Error> {
        let hooks = self.post_tool_exec_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                hook.on_post_tool_exec(ctx).await?;
            }
        }
        Ok(())
    }
}

// ==================== 实现 HookExecutor trait ====================

#[async_trait::async_trait]
impl caelix_api::context::HookExecutor for HookRegistry {
    async fn execute_message_update(
        &self,
        ctx: &caelix_api::hooks::MessageUpdateContext,
    ) -> Result<(), anyhow::Error> {
        // 委托给现有方法
        Self::execute_message_update(self, ctx).await
    }
    
    async fn execute_pre_tool_exec(
        &self,
        ctx: &mut caelix_api::hooks::PreToolExecContext,
    ) -> Result<(), anyhow::Error> {
        Self::execute_pre_tool_exec(self, ctx).await
    }
    
    async fn execute_post_tool_exec(
        &self,
        ctx: &mut caelix_api::hooks::PostToolExecContext,
    ) -> Result<(), anyhow::Error> {
        Self::execute_post_tool_exec(self, ctx).await
    }
}
