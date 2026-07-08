use crate::HookRegistry;
use crate::hooks::message_bus_hook::MessageBusHook;
use crate::hooks::skill_hook::SkillHook;
use crate::hooks::tool_result_check_hook::ToolResultSizeCheckHook;
use caelix_config::managers::SkillManager;
use std::sync::Arc;

/// Hook加载器
/// 负责预定义钩子的加载和注册
pub struct HookLoader;

impl HookLoader {
    /// 加载所有内置钩子
    ///
    /// # Arguments
    /// * `hook_registry` - 钩子注册中心
    /// * `skill_manager` - 技能管理器（用于SkillHook）
    pub async fn load_builtin_hooks(
        hook_registry: &HookRegistry,
        skill_manager: Arc<SkillManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 注册技能钩子
        let skill_hook = Arc::new(SkillHook::new(skill_manager));
        hook_registry.register_hook(skill_hook).await;

        // 注册消息总线钩子
        let message_bus_hook = Arc::new(MessageBusHook::new());
        hook_registry.register_hook(message_bus_hook).await;

        // 注册工具结果检查钩子
        let tool_result_check_hook = Arc::new(ToolResultSizeCheckHook::new());
        hook_registry.register_hook(tool_result_check_hook).await;

        tracing::info!(
            hook_count = hook_registry.hook_count().await,
            "Built-in hooks loaded"
        );
        Ok(())
    }
}
