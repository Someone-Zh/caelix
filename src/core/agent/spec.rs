use serde::{Deserialize, Serialize};
use crate::core::Tool;

/// 智能体蓝图，定义智能体的基本信息和能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    /// 智能体唯一标识
    pub name: String,
    /// 智能体元数据或描述
    pub metadata: AgentMetadata,
    /// 智能体系统提示（人设/边界）
    pub system_prompt: String,
    /// 智能体可用工具列表
    pub tools: Vec<Box<dyn Tool>>,
}

/// 智能体元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// 智能体描述
    pub description: String,
    /// 智能体版本
    pub version: String,
    /// 智能体作者
    pub author: Option<String>,
    /// 智能体标签
    pub tags: Vec<String>,
}

impl AgentSpec {
    /// 创建新的智能体蓝图
    pub fn new(
        name: String,
        metadata: AgentMetadata,
        system_prompt: String,
        tools: Vec<Box<dyn Tool>>,
    ) -> Self {
        Self {
            name,
            metadata,
            system_prompt,
            tools,
        }
    }
}