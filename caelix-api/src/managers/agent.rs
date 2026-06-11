//! AgentManager - 智能体管理器
//!
//! 管理所有已注册的 Agent 实例，不再保存 AgentSpec 而是保存 Arc<dyn Agent>

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent::Agent;
use thiserror::Error;

/// Agent 引用类型别名
type AgentRef = Arc<dyn Agent>;

/// 智能体注册中心错误
#[derive(Debug, Error)]
pub enum AgentRegistryError {
    #[error("Agent with name '{0}' already exists")]
    AgentAlreadyExists(String),
    #[error("Failed to load agent: {0}")]
    LoadError(String),
}

/// 智能体注册中心，负责维护所有智能体实例的索引
pub struct AgentManager {
    agents: Arc<RwLock<HashMap<String, AgentRef>>>,
}

impl Clone for AgentManager {
    fn clone(&self) -> Self {
        Self {
            agents: self.agents.clone(),
        }
    }
}

impl std::fmt::Debug for AgentManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let agent_names = self
            .agents
            .try_read()
            .map(
                |guard: tokio::sync::RwLockReadGuard<'_, HashMap<String, AgentRef>>| {
                    guard.keys().cloned().collect::<Vec<_>>()
                },
            )
            .unwrap_or_default();
        f.debug_struct("AgentManager")
            .field("agent_names", &agent_names)
            .finish()
    }
}

impl AgentManager {
    /// 创建新的智能体注册中心
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册智能体实例
    pub async fn register(&self, agent: AgentRef) -> Result<(), AgentRegistryError> {
        let spec = agent.get_spec();
        let name = spec.name.clone();
        let mut agents: tokio::sync::RwLockWriteGuard<'_, HashMap<String, AgentRef>> =
            self.agents.write().await;
        if agents.contains_key(&name) {
            return Err(AgentRegistryError::AgentAlreadyExists(name));
        }
        agents.insert(name, agent);
        Ok(())
    }

    /// 根据名称获取智能体实例
    pub async fn get(&self, name: &str) -> Option<AgentRef> {
        let agents: tokio::sync::RwLockReadGuard<'_, HashMap<String, AgentRef>> =
            self.agents.read().await;
        agents.get(name).cloned()
    }

    /// 获取所有智能体实例
    pub async fn get_all(&self) -> Vec<AgentRef> {
        let agents: tokio::sync::RwLockReadGuard<'_, HashMap<String, AgentRef>> =
            self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// 移除智能体
    #[allow(dead_code)]
    pub async fn remove(&self, name: &str) -> Option<AgentRef> {
        let mut agents: tokio::sync::RwLockWriteGuard<'_, HashMap<String, AgentRef>> =
            self.agents.write().await;
        agents.remove(name)
    }

    /// 获取所有注册的 agent 名称列表
    pub async fn list_all_names(&self) -> Vec<String> {
        let agents: tokio::sync::RwLockReadGuard<'_, HashMap<String, AgentRef>> =
            self.agents.read().await;
        agents.keys().cloned().collect()
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}
