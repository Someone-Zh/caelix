pub mod skill_hook;
pub mod loader;

use crate::base::agent::{AgentSpec, AgentOutputChunk};
use crate::base::provider::ChatMessage;
use async_trait::async_trait;
use bitflags::bitflags;
use std::sync::Arc;

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
    }
}

/// Hook作用范围类型
#[derive(Debug, Clone)]
#[allow(dead_code)] // 公共API，为将来扩展预留
pub enum HookScopeType {
    Name(String),      // 按Agent名称匹配
    Group(String),     // 按Agent组匹配
}

/// Hook作用范围模式
#[derive(Debug, Clone)]
#[allow(dead_code)] // 公共API，为将来扩展预留
pub enum HookScopeMode {
    Include,           // 仅对匹配的Agent生效
    Exclude,           // 对匹配的Agent不生效
}

/// Hook作用范围配置
#[derive(Debug, Clone)]
pub struct HookScope {
    #[allow(dead_code)] // 公共API，为将来扩展预留
    pub mode: HookScopeMode,
    #[allow(dead_code)] // 公共API，为将来扩展预留
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
    #[allow(dead_code)] // 公共API，为将来扩展预留
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

/// 基础上下文，所有阶段共享
#[derive(Debug, Clone)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct BaseContext {
    pub session_id: String,
    pub request_id: String,
    pub span_id: String,
    pub agent_name: String,
    pub agent_group: Option<String>,
}

/// Init阶段上下文
#[derive(Debug)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct InitContext<'a> {
    pub base: BaseContext,
    pub agent_spec: &'a mut AgentSpec,
}

/// Pre阶段上下文
#[derive(Debug)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct PreContext<'a> {
    pub base: BaseContext,
    pub messages: &'a mut Vec<ChatMessage>, // 改为可变引用，避免克隆
}

/// Post阶段上下文
#[derive(Debug)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct PostContext<'a> {
    pub base: BaseContext,
    pub input_messages: &'a [ChatMessage], // 改为切片引用，避免克隆
    pub output_chunks: &'a [AgentOutputChunk], // 改为切片引用，避免克隆
}

/// Hook阶段枚举
#[derive(Debug, Clone)]
#[allow(dead_code)] // 公共API，为将来扩展预留
pub enum HookStage {
    Init,
    Pre,
    Post,
}

/// Error阶段上下文
#[derive(Debug)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct ErrorContext {
    pub base: BaseContext,
    pub error: anyhow::Error,
    pub stage: HookStage,  // 标识在哪个阶段出错
}

/// 消息更新上下文
#[derive(Debug, Clone)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct MessageUpdateContext {
    pub base: BaseContext,
    pub messages: Arc<Vec<ChatMessage>>, // 使用Arc避免克隆
}

/// 工具执行前上下文
#[derive(Debug, Clone)]
#[allow(dead_code)] // 在异步闭包中使用
pub struct PreToolExecContext {
    pub base: BaseContext,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
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
    #[allow(dead_code)] // trait方法，由实现者使用
    fn scope(&self) -> &HookScope {
        // 默认实现：对所有Agent生效
        use std::sync::LazyLock;
        static DEFAULT_SCOPE: LazyLock<HookScope> = LazyLock::new(HookScope::default);
        &DEFAULT_SCOPE
    }
    
    /// 判断是否对指定Agent生效
    #[allow(dead_code)] // trait方法，由实现者使用
    fn should_apply(&self, agent_name: &str, agent_group: Option<&str>) -> bool {
        self.scope().matches(agent_name, agent_group)
    }
    
    /// Init-Process钩子：Agent初始化时调用（仅一次）
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_init(&self, _ctx: &mut InitContext<'_>) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Pre-Process钩子：Agent执行前调用，可修改输入消息
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_pre_process(&self, _ctx: &mut PreContext<'_>) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Post-Process钩子：Agent执行后调用，只读输出
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_post_process(&self, _ctx: &PostContext<'_>) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// On-Error钩子：出错时调用
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_error(&self, _ctx: &ErrorContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// Pre-Tool-Execution钩子：工具执行前调用
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_pre_tool_exec(&self, _ctx: &mut PreToolExecContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
    
    /// On-Message-Update钩子：消息更新时调用
    #[allow(dead_code)] // trait方法，由实现者使用
    async fn on_message_update(&self, _ctx: &MessageUpdateContext) -> Result<(), anyhow::Error> {
        Ok(())  // 默认空实现
    }
}
