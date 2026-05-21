//! Hook definitions for the API layer

use async_trait::async_trait;
use std::fmt;
use bitflags::bitflags;
use crate::agent::{AgentSpec, AgentOutputChunk};
use crate::provider::ChatMessage;
use crate::tool::ToolResult;

// Hook能力声明 - 位标志枚举
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HookCapability: u32 {
        const INIT = 1 << 0;              // Agent初始化阶段
        const PRE_PROCESS = 1 << 1;       // Agent执行前阶段
        const POST_PROCESS = 1 << 2;      // Agent执行后阶段
        const ERROR = 1 << 3;             // 错误处理阶段
        const PRE_TOOL_EXEC = 1 << 4;     // 工具执行前阶段（新增）
        const ON_MESSAGE_UPDATE = 1 << 5; // 消息更新时阶段（新增）
        const POST_TOOL_EXEC = 1 << 6;    // 工具执行后阶段（可修改结果）
    }
}

/// Hook作用范围类型
#[derive(Debug, Clone)]
pub enum HookScopeType {
    Name(String),      // 按Agent名称匹配
    Group(String),     // 按Agent组匹配
}

/// Hook作用范围模式
#[derive(Debug, Clone)]
pub enum HookScopeMode {
    Include,           // 仅对匹配的Agent生效
    Exclude,           // 对匹配的Agent不生效
}

/// Hook作用范围配置
#[derive(Debug, Clone)]
pub struct HookScope {
    pub mode: HookScopeMode,
    pub targets: Vec<HookScopeType>,
}

impl HookScope {
    /// 默认构造：无限制，对所有Agent生效
    pub fn default() -> Self {
        Self {
            mode: HookScopeMode::Include,
            targets: vec![],
        }
    }
    
    /// 判断Hook是否对指定Agent生效
    pub fn matches(&self, agent_name: &str, agent_group: Option<&str>) -> bool {
        if self.targets.is_empty() {
            return true; // 无限制，全部生效
        }
        
        let matched = self.targets.iter().any(|target| {
            match target {
                HookScopeType::Name(name) => name == agent_name,
                HookScopeType::Group(group) => agent_group == Some(group.as_str()),
            }
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

/// 基础上下文，所有阶段共享
#[derive(Debug, Clone)]
pub struct BaseContext {
    pub session_id: String,
    pub request_id: String,
    pub span_id: String,
    pub agent_name: String,
    pub agent_group: Option<String>,
}

/// 消息更新上下文
#[derive(Debug, Clone)]
pub struct MessageUpdateContext {
    pub base: BaseContext,
    pub messages: std::sync::Arc<Vec<crate::provider::ChatMessage>>,
}

/// 工具执行前上下文
#[derive(Debug, Clone)]
pub struct PreToolExecContext {
    pub base: BaseContext,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
}

/// 工具执行后上下文（可修改结果）
#[derive(Debug, Clone)]
pub struct PostToolExecContext {
    pub base: BaseContext,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub tool_result: ToolResult,
}

/// Init阶段上下文
pub struct InitContext<'a> {
    pub base: BaseContext,
    pub agent_spec: &'a mut AgentSpec,
}

/// Pre阶段上下文
pub struct PreContext<'a> {
    pub base: BaseContext,
    pub messages: &'a mut Vec<ChatMessage>,
}

/// Post阶段上下文
pub struct PostContext<'a> {
    pub base: BaseContext,
    pub input_messages: &'a [ChatMessage],
    pub output_chunks: &'a [AgentOutputChunk],
}

/// Hook阶段枚举
#[derive(Debug, Clone)]
pub enum HookStage {
    Init,
    Pre,
    Post,
}

/// Error阶段上下文
pub struct ErrorContext {
    pub base: BaseContext,
    pub error: anyhow::Error,
    pub stage: HookStage,
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
        Ok(())  // 默认空实现
    }
    
    /// Pre-Process钩子：Agent执行前调用，可修改输入消息
    async fn on_pre_process(&self, _ctx: &mut PreContext<'_>) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Post-Process钩子：Agent执行后调用，只读输出
    async fn on_post_process(&self, _ctx: &PostContext<'_>) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// On-Error钩子：出错时调用
    async fn on_error(&self, _ctx: &ErrorContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Pre-Tool-Execution钩子：工具执行前调用
    async fn on_pre_tool_exec(&self, _ctx: &mut PreToolExecContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Post-Tool-Execution钩子：工具执行后调用，可修改结果
    async fn on_post_tool_exec(&self, _ctx: &mut PostToolExecContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// On-Message-Update钩子：消息更新时调用
    async fn on_message_update(&self, _ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
}
