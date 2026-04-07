use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::base::agent::traits::AgentSpec;

/// 智能体注册中心，负责维护所有智能体蓝图的索引
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, AgentSpec>>>,
}

impl AgentRegistry {
    /// 创建新的智能体注册中心
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册智能体蓝图
    pub async fn register(&self, agent_spec: AgentSpec) -> Result<(), AgentRegistryError> {
        let mut agents = self.agents.write().await;
        if agents.contains_key(&agent_spec.name) {
            return Err(AgentRegistryError::AgentAlreadyExists(agent_spec.name));
        }
        agents.insert(agent_spec.name.clone(), agent_spec);
        Ok(())
    }

    /// 根据名称获取智能体蓝图
    pub async fn get(&self, name: &str) -> Option<AgentSpec> {
        let agents = self.agents.read().await;
        agents.get(name).cloned()
    }

    /// 获取所有智能体蓝图
    pub async fn get_all(&self) -> Vec<AgentSpec> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// 移除智能体蓝图
    pub async fn remove(&self, name: &str) -> Option<AgentSpec> {
        let mut agents = self.agents.write().await;
        agents.remove(name)
    }
}

/// 智能体注册中心错误
#[derive(Debug, thiserror::Error)]
pub enum AgentRegistryError {
    #[error("Agent with name '{0}' already exists")]
    AgentAlreadyExists(String),
}