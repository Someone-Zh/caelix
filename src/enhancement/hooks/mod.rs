pub mod skill_hook;

use crate::base::agent::AgentSpec;

/// Agent增强钩子trait
/// 允许在Agent执行前动态修改AgentSpec
pub trait AgentHook: Send + Sync {
    /// 钩子名称
    fn name(&self) -> &str;
    
    /// 在Agent执行前增强AgentSpec
    /// 可以修改系统提示词、添加工具等
    #[allow(dead_code)] // trait方法，由实现者使用
    fn enhance_agent(&self, agent_spec: &mut AgentSpec);
}
