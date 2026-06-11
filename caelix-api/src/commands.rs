//! Command definitions for the API layer

use serde::{Deserialize, Serialize};
use std::fmt;

/// 命令类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum CommandType {
    #[default]
    Prompt, // 提示词
    Shell, // Shell 命令
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Prompt => write!(f, "prompt"),
            CommandType::Shell => write!(f, "shell"),
        }
    }
}

/// 命令结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub command_type: CommandType,
    pub content: String,
}

impl Command {
    pub fn new(
        name: String,
        description: String,
        command_type: CommandType,
        content: String,
    ) -> Self {
        Self {
            name,
            description,
            command_type,
            content,
        }
    }
}
