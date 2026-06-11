use crate::tools::{DelegateTaskTool, create_all_builtin_tools};
use anyhow::anyhow;
use caelix_api::agent::Agent;
use caelix_api::plugins::{
    CommandSpec, NamedLlmProvider, Plugin, PluginCapability, PluginFactoryContext,
    PluginRegistration, SkillDef,
};
use caelix_api::provider::{LlmProvider, LlmType};
use caelix_api::tool::Tool;
use caelix_runtime::context::CaelixContext;
use std::sync::Arc;

struct DefaultServicePlugin {
    context: Arc<CaelixContext>,
}

impl DefaultServicePlugin {
    fn new(context: Arc<CaelixContext>) -> Self {
        Self { context }
    }
}

#[async_trait::async_trait]
impl Plugin for DefaultServicePlugin {
    fn name(&self) -> &str {
        "caelix-service-default"
    }

    fn capabilities(&self) -> PluginCapability {
        PluginCapability::LLM_PROVIDER
            | PluginCapability::TOOL
            | PluginCapability::SKILL
            | PluginCapability::AGENT
            | PluginCapability::COMMAND
    }

    async fn llm_providers(&self) -> anyhow::Result<Vec<NamedLlmProvider>> {
        let configs = caelix_config::provider_loader::load_provider_configs(
            &self.context.env_config.caelix_home,
        )
        .map_err(|e| anyhow!(e.to_string()))?;

        let mut providers = Vec::new();
        for (key, config) in configs {
            let name = if config.name.is_empty() {
                key
            } else {
                config.name.clone()
            };

            let provider: Arc<dyn LlmProvider> = match config.llm_type {
                LlmType::OpenAI => Arc::new(caelix_llm::OpenAIProvider::new(Arc::new(config))),
            };
            providers.push(NamedLlmProvider::new(name, provider));
        }

        Ok(providers)
    }

    async fn tools(&self) -> anyhow::Result<Vec<Arc<dyn Tool>>> {
        let mut tools = create_all_builtin_tools();
        tools.push(Arc::new(DelegateTaskTool::new(self.context.clone())));
        Ok(tools)
    }

    async fn skills(&self) -> anyhow::Result<Vec<SkillDef>> {
        let skills_dir = self.context.env_config.caelix_home.join("skills");
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir)?;
            println!("Creating skills directory at: {:?}", skills_dir);
        }

        let skills =
            caelix_config::skills_loader::load_skills_from_directory(&skills_dir.to_string_lossy())
                .await
                .map_err(|e| anyhow!(e))?;
        Ok(skills
            .into_iter()
            .map(|skill| {
                SkillDef::new(
                    skill.name,
                    skill.namespace,
                    skill.description,
                    skill.content,
                )
            })
            .collect())
    }

    async fn agent_instances(&self) -> anyhow::Result<Vec<Arc<dyn Agent>>> {
        let agents_dir = self.context.env_config.caelix_home.join("agents");
        if !agents_dir.exists() {
            std::fs::create_dir_all(&agents_dir)?;
            println!("Creating agents directory at: {:?}", agents_dir);
            println!("Please add .agent files to this directory");
        }

        let mut agents = caelix_config::agents_loader::load_agents_from_directory(
            &agents_dir.to_string_lossy(),
            &self.context.tool_manager,
        )
        .await
        .map_err(|e| anyhow!(e))?;

        for agent in &mut agents {
            self.context
                .hook_registry
                .apply_init_hooks(agent, Some("init"))
                .await?;
        }

        Ok(agents
            .into_iter()
            .map(|agent| {
                Arc::new(caelix_agent::loop_agent::LoopAgent::new(Arc::new(agent)))
                    as Arc<dyn Agent>
            })
            .collect())
    }

    async fn commands(&self) -> anyhow::Result<Vec<CommandSpec>> {
        let commands_dir = self.context.env_config.caelix_home.join("commands");
        if !commands_dir.exists() {
            std::fs::create_dir_all(&commands_dir)?;
            println!("Creating commands directory at: {:?}", commands_dir);
        }

        let commands = caelix_config::commands_loader::load_commands_from_directory(
            &commands_dir.to_string_lossy(),
        )
        .await
        .map_err(|e| anyhow!(e))?;
        Ok(commands)
    }
}

fn create_default_service_plugin(context: PluginFactoryContext) -> Arc<dyn Plugin> {
    let context = context
        .downcast::<CaelixContext>()
        .expect("default service plugin requires CaelixContext");
    Arc::new(DefaultServicePlugin::new(context))
}

inventory::submit! {
    PluginRegistration::new("caelix-service-default", create_default_service_plugin)
}
