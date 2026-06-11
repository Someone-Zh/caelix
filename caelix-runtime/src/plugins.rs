use crate::hooks::message_bus_hook::MessageBusHook;
use crate::hooks::skill_hook::SkillHook;
use crate::hooks::tool_result_check_hook::ToolResultSizeCheckHook;
use caelix_api::hooks::AgentHook;
use caelix_api::plugins::{Plugin, PluginCapability, PluginFactoryContext, PluginRegistration};
use std::sync::Arc;

struct RuntimeHookPlugin {
    context: Arc<crate::context::CaelixContext>,
}

impl RuntimeHookPlugin {
    fn new(context: Arc<crate::context::CaelixContext>) -> Self {
        Self { context }
    }
}

#[async_trait::async_trait]
impl Plugin for RuntimeHookPlugin {
    fn name(&self) -> &str {
        "caelix-runtime-hooks"
    }

    fn capabilities(&self) -> PluginCapability {
        PluginCapability::HOOK
    }

    async fn agent_hooks(&self) -> anyhow::Result<Vec<Arc<dyn AgentHook>>> {
        Ok(vec![
            Arc::new(SkillHook::new(self.context.skill_manager.clone())),
            Arc::new(MessageBusHook::new()),
            Arc::new(ToolResultSizeCheckHook::new()),
        ])
    }
}

fn create_runtime_hook_plugin(context: PluginFactoryContext) -> Arc<dyn Plugin> {
    let context = context
        .downcast::<crate::context::CaelixContext>()
        .expect("runtime hook plugin requires CaelixContext");
    Arc::new(RuntimeHookPlugin::new(context))
}

inventory::submit! {
    PluginRegistration::new("caelix-runtime-hooks", create_runtime_hook_plugin)
}
