use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use crate::base::tool::traits::ToolDefinition;
use crate::base::Tool;

// ==============================
// 可序列化配置（存JSON/数据库）
// ==============================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    pub metadata: AgentMetadata,
    pub system_prompt: String,
    pub tool_definitions: Vec<ToolDefinition>,
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
}

// ==============================
// 运行时实例（手动实现 Clone）
// ==============================
#[derive(Debug)]
pub struct AgentRuntime {
    pub spec: AgentSpec,
    pub tools: Vec<Box<dyn Tool>>,
}

// 手动实现 Clone，用 tool.clone_box() 复制每个工具
impl Clone for AgentRuntime {
    fn clone(&self) -> Self {
        let cloned_tools = self.tools
            .iter()
            .map(|tool| tool.clone_box()) 
            .collect();

        Self {
            spec: self.spec.clone(),
            tools: cloned_tools,
        }
    }
}

impl AgentSpec {
    pub fn new(
        name: String,
        metadata: AgentMetadata,
        system_prompt: String,
        tools: &[Box<dyn Tool>],
    ) -> Self {
        let tool_definitions = tools
            .iter()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters_schema: tool.parameters_schema(),
            })
            .collect();

        Self {
            name,
            metadata,
            system_prompt,
            tool_definitions,
        }
    }

    pub fn with_runtime_tools(self, tools: Vec<Box<dyn Tool>>) -> AgentRuntime {
        AgentRuntime { spec: self, tools }
    }
}