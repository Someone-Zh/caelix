use serde::{Deserialize, Serialize};

/// 命令种类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandType {
    Prompt,   // 提示词
    Shell,    // Shell 命令
}

impl Default for CommandType {
    fn default() -> Self {
        Self::Prompt
    }
}

/// 命令定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub r#type: CommandType,
    pub content: String,
}

impl Command {
    pub fn new(name: String, description: String, r#type: CommandType, content: String) -> Self {
        Self {
            name,
            description,
            r#type,
            content,
        }
    }
}
