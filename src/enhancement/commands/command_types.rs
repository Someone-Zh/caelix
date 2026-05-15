use serde::{Deserialize, Serialize};

/// 命令种类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub enum CommandType {
    #[default]
    Prompt,   // 提示词
    Shell,    // Shell 命令
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
