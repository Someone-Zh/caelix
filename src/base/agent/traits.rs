use crate::base::tool::traits::ToolDefinition;
use crate::base::Tool;

#[derive(Debug)]
pub struct AgentSpec {
    pub name: String,
    pub system_prompt: String,
    pub tools: Vec<Box<dyn Tool>>,
}

impl AgentSpec {
    pub fn new(
        name: String,
        system_prompt: String,
        tools: Vec<Box<dyn Tool>>,
    ) -> Self {

        Self {
            name,
            system_prompt,
            tools: tools,
        }
    }
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.to_definition()).collect()
    }
}