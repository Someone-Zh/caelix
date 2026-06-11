//! Plugin definitions for the API layer.

use crate::agent::{Agent, AgentSpec};
use crate::commands::Command;
use crate::hooks::{AgentHook, Hook};
use crate::provider::LlmProvider;
use crate::tool::Tool;
use async_trait::async_trait;
use bitflags::bitflags;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type CommandSpec = Command;
pub type PluginFactoryContext = Arc<dyn Any + Send + Sync>;
pub type PluginFactory = fn(PluginFactoryContext) -> Arc<dyn Plugin>;

#[derive(Debug, Clone)]
pub struct SkillDef {
    pub name: String,
    pub namespace: String,
    pub description: String,
    pub content: String,
}

impl SkillDef {
    pub fn new(
        name: impl Into<String>,
        namespace: impl Into<String>,
        description: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            description: description.into(),
            content: content.into(),
        }
    }
}

bitflags! {
    /// Plugin capability declaration by initialization phase.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PluginCapability: u32 {
        const LLM_PROVIDER = 1 << 0;
        const TOOL = 1 << 1;
        const SKILL = 1 << 2;
        const AGENT = 1 << 3;
        const COMMAND = 1 << 4;
        const HOOK = 1 << 5;
    }
}

#[derive(Debug, Clone)]
pub struct NamedLlmProvider {
    pub name: String,
    pub provider: Arc<dyn LlmProvider>,
}

impl NamedLlmProvider {
    pub fn new(name: impl Into<String>, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            name: name.into(),
            provider,
        }
    }
}

pub enum PluginContribution {
    LlmProvider(Arc<dyn LlmProvider>),
    NamedLlmProvider(NamedLlmProvider),
    Tool(Arc<dyn Tool>),
    Skill(SkillDef),
    Agent(Arc<dyn Agent>),
    AgentSpec(AgentSpec),
    Command(CommandSpec),
    AgentHook(Arc<dyn AgentHook>),
    Hook(Box<dyn Hook>),
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;

    fn capabilities(&self) -> PluginCapability {
        PluginCapability::all()
    }

    async fn llm_providers(&self) -> anyhow::Result<Vec<NamedLlmProvider>> {
        Ok(Vec::new())
    }

    async fn tools(&self) -> anyhow::Result<Vec<Arc<dyn Tool>>> {
        Ok(Vec::new())
    }

    async fn skills(&self) -> anyhow::Result<Vec<SkillDef>> {
        Ok(Vec::new())
    }

    async fn agents(&self) -> anyhow::Result<Vec<AgentSpec>> {
        Ok(Vec::new())
    }

    async fn agent_instances(&self) -> anyhow::Result<Vec<Arc<dyn Agent>>> {
        Ok(Vec::new())
    }

    async fn commands(&self) -> anyhow::Result<Vec<CommandSpec>> {
        Ok(Vec::new())
    }

    async fn agent_hooks(&self) -> anyhow::Result<Vec<Arc<dyn AgentHook>>> {
        Ok(Vec::new())
    }

    async fn hooks(&self) -> anyhow::Result<Vec<Box<dyn Hook>>> {
        Ok(Vec::new())
    }
}

#[derive(Clone)]
pub struct PluginRegistration {
    pub name: &'static str,
    pub factory: PluginFactory,
}

impl PluginRegistration {
    pub const fn new(name: &'static str, factory: PluginFactory) -> Self {
        Self { name, factory }
    }
}

inventory::collect!(PluginRegistration);

pub fn inventory_plugins(context: PluginFactoryContext) -> Vec<Arc<dyn Plugin>> {
    inventory::iter::<PluginRegistration>
        .into_iter()
        .map(|registration| (registration.factory)(context.clone()))
        .collect()
}

#[derive(Clone, Default)]
pub struct PluginRegistry {
    plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    llm_provider_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    tool_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    skill_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    agent_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    command_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
    hook_plugins: Arc<RwLock<Vec<Arc<dyn Plugin>>>>,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry")
            .field(
                "plugin_count",
                &self.plugins.try_read().map(|p| p.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
pub trait PluginManager: Send + Sync {
    async fn register_plugin(&self, plugin: Arc<dyn Plugin>);
    async fn register_plugins(&self, plugins: Vec<Arc<dyn Plugin>>);

    async fn all_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn llm_provider_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn tool_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn skill_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn agent_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn command_plugins(&self) -> Vec<Arc<dyn Plugin>>;
    async fn hook_plugins(&self) -> Vec<Arc<dyn Plugin>>;
}

#[async_trait]
impl PluginManager for PluginRegistry {
    async fn register_plugin(&self, plugin: Arc<dyn Plugin>) {
        let capabilities = plugin.capabilities();
        self.plugins.write().await.push(plugin.clone());

        if capabilities.contains(PluginCapability::LLM_PROVIDER) {
            self.llm_provider_plugins.write().await.push(plugin.clone());
        }
        if capabilities.contains(PluginCapability::TOOL) {
            self.tool_plugins.write().await.push(plugin.clone());
        }
        if capabilities.contains(PluginCapability::SKILL) {
            self.skill_plugins.write().await.push(plugin.clone());
        }
        if capabilities.contains(PluginCapability::AGENT) {
            self.agent_plugins.write().await.push(plugin.clone());
        }
        if capabilities.contains(PluginCapability::COMMAND) {
            self.command_plugins.write().await.push(plugin.clone());
        }
        if capabilities.contains(PluginCapability::HOOK) {
            self.hook_plugins.write().await.push(plugin);
        }
    }

    async fn register_plugins(&self, plugins: Vec<Arc<dyn Plugin>>) {
        for plugin in plugins {
            self.register_plugin(plugin).await;
        }
    }

    async fn all_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.plugins.read().await.clone()
    }

    async fn llm_provider_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.llm_provider_plugins.read().await.clone()
    }

    async fn tool_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.tool_plugins.read().await.clone()
    }

    async fn skill_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.skill_plugins.read().await.clone()
    }

    async fn agent_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.agent_plugins.read().await.clone()
    }

    async fn command_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.command_plugins.read().await.clone()
    }

    async fn hook_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.hook_plugins.read().await.clone()
    }
}
