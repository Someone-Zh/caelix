use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use super::Tool;

pub struct ToolManager {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl std::fmt::Debug for ToolManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // Only show the structure and tool names, not the tools themselves since they don't implement Debug
            let tool_names = self.tools.try_read()
                .map(|lock| lock.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            f.debug_struct("ToolManager")
                .field("tool_names", &tool_names)
                .finish()
        }
    }

    impl Default for ToolManager {
        fn default() -> Self {
            Self {
                tools: RwLock::new(HashMap::new()),
            }
        }
    }

impl ToolManager {
    pub fn new() -> Self {
        Self::default()
    }

    // 注册工具，同名工具会覆盖已有的
    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.write().await.insert(name, tool);
    }

    // 根据名称获取工具
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().await.get(name).cloned()
    }

    // 获取所有工具列表
    pub async fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.read().await.values().cloned().collect()
    }

    // 获取所有工具名称列表
    pub async fn list_names(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }
}