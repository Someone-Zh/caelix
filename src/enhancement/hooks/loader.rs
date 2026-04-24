use std::sync::Arc;
use crate::enhancement::hooks::skill_hook::SkillHook;
use crate::manager::SkillManager;
use crate::enhancement::HookRegistry;

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
        
        println!("Built-in hooks loaded. Total hooks: {}", hook_registry.hook_count().await);
        Ok(())
    }
}
