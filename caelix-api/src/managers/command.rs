//! CommandManager - 命令管理器

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::commands::{Command, CommandType};

#[derive(Debug, Clone)]
pub struct CommandManager {
    commands: Arc<RwLock<Vec<Command>>>,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册命令
    pub async fn register(&self, command: Command) {
        let mut commands = self.commands.write().await;
        commands.push(command);
    }

    /// 批量注册命令
    pub async fn register_batch(&self, commands: Vec<Command>) {
        let mut all_commands = self.commands.write().await;
        all_commands.extend(commands);
    }

    /// 获取所有命令
    pub async fn get_all(&self) -> Vec<Command> {
        let commands = self.commands.read().await;
        commands.clone()
    }

    /// 根据名称获取命令
    pub async fn get_by_name(&self, name: &str) -> Option<Command> {
        let commands = self.commands.read().await;
        commands.iter().find(|c| c.name == name).cloned()
    }

    /// 根据类型过滤命令
    pub async fn get_by_type(&self, cmd_type: &CommandType) -> Vec<Command> {
        let commands = self.commands.read().await;
        commands
            .iter()
            .filter(|c| c.command_type == *cmd_type)
            .cloned()
            .collect()
    }
}

impl Default for CommandManager {
    fn default() -> Self {
        Self::new()
    }
}
