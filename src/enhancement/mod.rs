pub mod hooks;
pub mod commands;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::enhancement::hooks::{AgentHook, InitContext, PreContext, PostContext, ErrorContext, BaseContext, HookCapability, PreToolExecContext, MessageUpdateContext};
use crate::base::agent::AgentSpec;

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
        if caps.contains(HookCapability::ON_MESSAGE_UPDATE) {
            self.message_update_hooks.write().await.push(hook.clone());
        }
    }

    /// 执行Init阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_init(&self, ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        let hooks = self.init_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "init",
                            "agent_name": ctx.base.agent_name
                        })
                    );
                }
                
                println!("Executing init hook: {}", hook.name());
                hook.on_init(ctx).await?;
                
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_complete",
                            "hook_name": hook.name(),
                            "stage": "init",
                            "result": "success"
                        })
                    );
                }
            }
        }
        Ok(())
    }

    /// 执行Pre阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_pre(&self, ctx: &mut PreContext<'_>) -> Result<(), anyhow::Error> {
        let hooks = self.pre_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "pre",
                            "agent_name": ctx.base.agent_name
                        })
                    );
                }
                
                println!("Executing pre-process hook: {}", hook.name());
                hook.on_pre_process(ctx).await?;
                
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_complete",
                            "hook_name": hook.name(),
                            "stage": "pre",
                            "result": "success"
                        })
                    );
                }
            }
        }
        Ok(())
    }

    /// 执行Post阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_post(&self, ctx: &PostContext<'_>) -> Result<(), anyhow::Error> {
        let hooks = self.post_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "post",
                            "agent_name": ctx.base.agent_name
                        })
                    );
                }
                
                println!("Executing post-process hook: {}", hook.name());
                hook.on_post_process(ctx).await?;
                
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_complete",
                            "hook_name": hook.name(),
                            "stage": "post",
                            "result": "success"
                        })
                    );
                }
            }
        }
        Ok(())
    }

    /// 执行Error阶段钩子
    #[allow(dead_code)] // 在异步闭包中使用
    pub async fn execute_error(&self, ctx: &ErrorContext) -> Result<(), anyhow::Error> {
        let hooks = self.error_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "error",
                            "agent_name": ctx.base.agent_name
                        })
                    );
                }
                
                println!("Executing error hook: {}", hook.name());
                // Error钩子失败不中断，只记录日志
                if let Err(e) = hook.on_error(ctx).await {
                    eprintln!("Error hook {} failed: {:?}", hook.name(), e);
                    
                    #[cfg(feature = "logging")]
                    {
                        crate::debug_log!(
                            "ERROR",
                            &ctx.base.session_id,
                            &ctx.base.request_id,
                            &ctx.base.span_id,
                            &format!("mod.rs:{}", line!()),
                            serde_json::json!({
                                "event": "hook_execute_failed",
                                "hook_name": hook.name(),
                                "stage": "error",
                                "error": format!("{:?}", e)
                            })
                        );
                    }
                } else {
                    #[cfg(feature = "logging")]
                    {
                        crate::debug_log!(
                            "DEBUG",
                            &ctx.base.session_id,
                            &ctx.base.request_id,
                            &ctx.base.span_id,
                            &format!("mod.rs:{}", line!()),
                            serde_json::json!({
                                "event": "hook_execute_complete",
                                "hook_name": hook.name(),
                                "stage": "error",
                                "result": "success"
                            })
                        );
                    }
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

    /// 执行Pre-Tool-Execution阶段钩子
    #[allow(dead_code)] // 公共API，供将来使用
    pub async fn execute_pre_tool_exec(&self, ctx: &mut PreToolExecContext) -> Result<(), anyhow::Error> {
        let hooks = self.pre_tool_exec_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "pre_tool_exec",
                            "agent_name": ctx.base.agent_name,
                            "tool_name": ctx.tool_name
                        })
                    );
                }
                
                println!("Executing pre-tool-exec hook: {}", hook.name());
                hook.on_pre_tool_exec(ctx).await?;
                
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_complete",
                            "hook_name": hook.name(),
                            "stage": "pre_tool_exec",
                            "result": "success"
                        })
                    );
                }
            }
        }
        Ok(())
    }

    /// 执行Message-Update阶段钩子
    #[allow(dead_code)] // 公共API，供将来使用
    pub async fn execute_message_update(&self, ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        let hooks = self.message_update_hooks.read().await;
        for hook in hooks.iter() {
            if hook.should_apply(&ctx.base.agent_name, ctx.base.agent_group.as_deref()) {
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_start",
                            "hook_name": hook.name(),
                            "stage": "message_update",
                            "agent_name": ctx.base.agent_name,
                            "message_count": ctx.messages.len()
                        })
                    );
                }
                
                println!("Executing message-update hook: {}", hook.name());
                hook.on_message_update(ctx).await?;
                
                #[cfg(feature = "logging")]
                {
                    crate::debug_log!(
                        "DEBUG",
                        &ctx.base.session_id,
                        &ctx.base.request_id,
                        &ctx.base.span_id,
                        &format!("mod.rs:{}", line!()),
                        serde_json::json!({
                            "event": "hook_execute_complete",
                            "hook_name": hook.name(),
                            "stage": "message_update",
                            "result": "success"
                        })
                    );
                }
            }
        }
        Ok(())
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
        let hooks = self.hooks.read().await;
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
                    agent_group: agent_spec.group.as_ref().map(|g| g.to_string()),  // Arc<String> -> Option<String>
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
}