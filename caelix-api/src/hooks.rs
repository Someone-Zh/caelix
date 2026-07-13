//! Hook definitions for the API layer

use crate::agent::AgentSpec;
use crate::tool::ToolResult;
use anyhow::Result;
use async_trait::async_trait;
use bitflags::bitflags;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

// Hook能力声明 - 位标志枚举
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HookCapability: u32 {
        const INIT = 1 << 0;              // Agent初始化阶段
        const PRE_TOOL_EXEC = 1 << 1;     // 工具执行前阶段
        const ON_MESSAGE_UPDATE = 1 << 2; // 消息更新时阶段
        const POST_TOOL_EXEC = 1 << 3;    // 工具执行后阶段（可修改结果）
    }
}

/// Hook作用范围类型
#[derive(Debug, Clone)]
pub enum HookScopeType {
    Name(String),  // 按Agent名称匹配
    Group(String), // 按Agent组匹配
}

/// Hook作用范围模式
#[derive(Debug, Clone)]
pub enum HookScopeMode {
    Include, // 仅对匹配的Agent生效
    Exclude, // 对匹配的Agent不生效
}

/// Hook作用范围配置
#[derive(Debug, Clone)]
pub struct HookScope {
    pub mode: HookScopeMode,
    pub targets: Vec<HookScopeType>,
}

impl Default for HookScope {
    fn default() -> Self {
        Self {
            mode: HookScopeMode::Include,
            targets: vec![],
        }
    }
}

impl HookScope {
    /// 判断Hook是否对指定Agent生效
    pub fn matches(&self, agent_name: &str, agent_group: Option<&str>) -> bool {
        if self.targets.is_empty() {
            return true; // 无限制，全部生效
        }

        let matched = self.targets.iter().any(|target| match target {
            HookScopeType::Name(name) => name == agent_name,
            HookScopeType::Group(group) => agent_group == Some(group.as_str()),
        });

        match self.mode {
            HookScopeMode::Include => matched,
            HookScopeMode::Exclude => !matched,
        }
    }
}

/// 钩子类型枚举
#[derive(Debug, Clone, PartialEq)]
pub enum HookType {
    /// 消息发送前钩子
    BeforeMessageSend,
    /// 消息接收后钩子
    AfterMessageReceive,
    /// 工具执行前钩子
    BeforeToolExecute,
    /// 工具执行后钩子
    AfterToolExecute,
    /// Agent 启动前钩子
    BeforeAgentStart,
    /// Agent 结束后钩子
    AfterAgentEnd,
}

impl fmt::Display for HookType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookType::BeforeMessageSend => write!(f, "before_message_send"),
            HookType::AfterMessageReceive => write!(f, "after_message_receive"),
            HookType::BeforeToolExecute => write!(f, "before_tool_execute"),
            HookType::AfterToolExecute => write!(f, "after_tool_execute"),
            HookType::BeforeAgentStart => write!(f, "before_agent_start"),
            HookType::AfterAgentEnd => write!(f, "after_agent_end"),
        }
    }
}

/// 钩子 Trait
#[async_trait]
pub trait Hook: Send + Sync {
    /// 获取钩子名称
    fn name(&self) -> &str;

    /// 获取钩子类型
    fn hook_type(&self) -> HookType;

    /// 执行钩子逻辑
    async fn execute(&self, context: &HookContext) -> Result<(), String>;
}

/// 钩子执行上下文
#[derive(Debug, Clone)]
pub struct HookContext {
    pub session_id: String,
    pub agent_name: Option<String>,
    pub message_content: Option<String>,
    pub tool_name: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl HookContext {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            agent_name: None,
            message_content: None,
            tool_name: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_agent(mut self, agent_name: &str) -> Self {
        self.agent_name = Some(agent_name.to_string());
        self
    }

    pub fn with_message(mut self, content: &str) -> Self {
        self.message_content = Some(content.to_string());
        self
    }

    pub fn with_tool(mut self, tool_name: &str) -> Self {
        self.tool_name = Some(tool_name.to_string());
        self
    }
}

/// 消息更新上下文
#[derive(Debug, Clone)]
pub struct MessageUpdateContext {
    pub messages: std::sync::Arc<Vec<crate::provider::ChatMessage>>,
    pub agent_name: String,
    pub agent_group: Option<String>,
}

/// 工具执行前上下文
#[derive(Debug, Clone)]
pub struct PreToolExecContext {
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub agent_name: String,
    pub agent_group: Option<String>,
}

/// 工具执行后上下文（可修改结果）
#[derive(Debug, Clone)]
pub struct PostToolExecContext {
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub tool_result: ToolResult,
    pub agent_name: String,
    pub agent_group: Option<String>,
}

/// Init阶段上下文
pub struct InitContext<'a> {
    pub agent_spec: &'a mut AgentSpec,
}

/// Agent增强钩子trait
/// 允许在Agent生命周期的不同阶段进行增强
#[async_trait]
pub trait AgentHook: Send + Sync {
    /// 钩子名称
    fn name(&self) -> &str;

    /// 声明该钩子关注的阶段（能力声明）
    /// 默认返回全部阶段，实现者可以重写以优化性能
    fn capabilities(&self) -> HookCapability {
        HookCapability::all()
    }

    /// 钩子作用范围
    fn scope(&self) -> &HookScope {
        // 默认实现：对所有Agent生效
        use std::sync::LazyLock;
        static DEFAULT_SCOPE: LazyLock<HookScope> = LazyLock::new(HookScope::default);
        &DEFAULT_SCOPE
    }

    /// 判断是否对指定Agent生效
    fn should_apply(&self, agent_name: &str, agent_group: Option<&str>) -> bool {
        self.scope().matches(agent_name, agent_group)
    }

    /// Init-Process钩子：Agent初始化时调用（仅一次）
    async fn on_init(&self, _ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        Ok(()) // 默认空实现
    }

    /// Pre-Tool-Execution钩子：工具执行前调用
    async fn on_pre_tool_exec(&self, _ctx: &mut PreToolExecContext) -> Result<(), anyhow::Error> {
        Ok(()) // 默认空实现
    }

    /// Post-Tool-Execution钩子：工具执行后调用，可修改结果
    async fn on_post_tool_exec(&self, _ctx: &mut PostToolExecContext) -> Result<(), anyhow::Error> {
        Ok(()) // 默认空实现
    }

    /// On-Message-Update钩子：消息更新时调用
    async fn on_message_update(&self, _ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        Ok(()) // 默认空实现
    }
}

/// 钩子引用类型别名
type HookRef = Arc<dyn AgentHook>;

/// 钩子注册中心
/// 管理所有Agent增强钩子，并在Agent生命周期的不同阶段应用它们
#[derive(Clone)]
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<HookRef>>>,
    init_hooks: Arc<RwLock<Vec<HookRef>>>,
    pre_tool_exec_hooks: Arc<RwLock<Vec<HookRef>>>,
    post_tool_exec_hooks: Arc<RwLock<Vec<HookRef>>>,
    message_update_hooks: Arc<RwLock<Vec<HookRef>>>,
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self
            .hooks
            .try_read()
            .map(|guard: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>>| guard.len())
            .unwrap_or(0);
        f.debug_struct("HookRegistry")
            .field("hook_count", &count)
            .finish()
    }
}

impl HookRegistry {
    /// 创建新的钩子注册中心
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::<HookRef>::new())),
            init_hooks: Arc::new(RwLock::new(Vec::<HookRef>::new())),
            pre_tool_exec_hooks: Arc::new(RwLock::new(Vec::<HookRef>::new())),
            post_tool_exec_hooks: Arc::new(RwLock::new(Vec::<HookRef>::new())),
            message_update_hooks: Arc::new(RwLock::new(Vec::<HookRef>::new())),
        }
    }

    /// 注册钩子
    pub async fn register_hook(&self, hook: HookRef) {
        let caps = hook.capabilities();
        {
            let mut guard: tokio::sync::RwLockWriteGuard<'_, Vec<HookRef>> =
                self.hooks.write().await;
            guard.push(hook.clone());
        }
        if caps.contains(HookCapability::INIT) {
            let mut guard: tokio::sync::RwLockWriteGuard<'_, Vec<HookRef>> =
                self.init_hooks.write().await;
            guard.push(hook.clone());
        }
        if caps.contains(HookCapability::PRE_TOOL_EXEC) {
            let mut guard: tokio::sync::RwLockWriteGuard<'_, Vec<HookRef>> =
                self.pre_tool_exec_hooks.write().await;
            guard.push(hook.clone());
        }
        if caps.contains(HookCapability::POST_TOOL_EXEC) {
            let mut guard: tokio::sync::RwLockWriteGuard<'_, Vec<HookRef>> =
                self.post_tool_exec_hooks.write().await;
            guard.push(hook.clone());
        }
        if caps.contains(HookCapability::ON_MESSAGE_UPDATE) {
            let mut guard: tokio::sync::RwLockWriteGuard<'_, Vec<HookRef>> =
                self.message_update_hooks.write().await;
            guard.push(hook.clone());
        }
    }

    /// 获取已注册的钩子数量
    pub async fn hook_count(&self) -> usize {
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> = self.hooks.read().await;
        hooks.len()
    }

    pub async fn list_hooks(&self) -> Vec<Arc<dyn AgentHook>> {
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> = self.hooks.read().await;
        hooks.iter().cloned().collect()
    }

    /// 应用Init阶段钩子到AgentSpec（用于Agent注册时的一次性增强）
    pub async fn apply_init_hooks(
        &self,
        agent_spec: &mut AgentSpec,
        session_id: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> = self.init_hooks.read().await;
        let _session_id = session_id.unwrap_or("init");

        for obj in hooks.iter() {
            let hook: &dyn AgentHook = &**obj;
            if hook.should_apply(
                &agent_spec.name,
                agent_spec.group.as_ref().map(|s| s.as_str()),
            ) {
                let mut init_ctx = InitContext { agent_spec };

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
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> =
            self.message_update_hooks.read().await;
        for obj in hooks.iter() {
            let hook: &dyn AgentHook = &**obj;
            if hook.should_apply(&ctx.agent_name, ctx.agent_group.as_deref()) {
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
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> =
            self.pre_tool_exec_hooks.read().await;
        for obj in hooks.iter() {
            let hook: &dyn AgentHook = &**obj;
            if hook.should_apply(&ctx.agent_name, ctx.agent_group.as_deref()) {
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
        let hooks: tokio::sync::RwLockReadGuard<'_, Vec<HookRef>> =
            self.post_tool_exec_hooks.read().await;
        for obj in hooks.iter() {
            let hook: &dyn AgentHook = &**obj;
            if hook.should_apply(&ctx.agent_name, ctx.agent_group.as_deref()) {
                hook.on_post_tool_exec(ctx).await?;
            }
        }
        Ok(())
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
