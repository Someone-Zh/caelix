use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::base::agent::AgentSpec;

/// 智能体注册中心，负责维护所有智能体蓝图的索引
#[derive(Debug, Clone)]
pub struct AgentManager {
    agents: Arc<RwLock<HashMap<String, Arc<AgentSpec>>>>,
}

impl AgentManager {
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
        agents.insert(agent_spec.name.clone(), Arc::new(agent_spec));
        Ok(())
    }

    /// 根据名称获取智能体蓝图
    pub async fn get(&self, name: &str) -> Option<Arc<AgentSpec>> {
        let agents = self.agents.read().await;
        agents.get(name).cloned()
    }

    /// 获取所有智能体蓝图
    pub async fn get_all(&self) -> Vec<Arc<AgentSpec>> {
        let agents = self.agents.read().await;
        let result = agents.values().cloned().collect();
        // 锁在此处自动释放
        result
    }

    /// 移除智能体蓝图
    #[allow(dead_code)] // 公共API，为将来使用预留
    pub async fn remove(&self, name: &str) -> Option<Arc<AgentSpec>> {
        let mut agents = self.agents.write().await;
        agents.remove(name)
    }

    /// 获取所有注册的 agent 名称列表
    pub async fn list_all_names(&self) -> Vec<String> {
        let agents = self.agents.read().await;
        agents.keys().cloned().collect()
    }

}

/// 智能体注册中心错误
#[derive(Debug, thiserror::Error)]
pub enum AgentRegistryError {
    #[error("Agent with name '{0}' already exists")]
    AgentAlreadyExists(String),
    #[error("Failed to load agent: {0}")]
    LoadError(String),
}