use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

/// Thread-safe manager for global variables and variables scoped by space.
#[derive(Clone, Debug, Default)]
pub struct VariableManager {
    global: Arc<RwLock<HashMap<String, String>>>,
    spaces: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

impl VariableManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set_global(&self, key: impl Into<String>, value: impl Into<String>) {
        self.global.write().await.insert(key.into(), value.into());
    }

    pub async fn get_global(&self, key: &str) -> Option<String> {
        self.global.read().await.get(key).cloned()
    }

    pub async fn delete_global(&self, key: &str) {
        self.global.write().await.remove(key);
    }

    pub async fn list_globals(&self) -> HashMap<String, String> {
        self.global.read().await.clone()
    }

    pub async fn set_space_var(
        &self,
        space: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) {
        let mut spaces = self.spaces.write().await;
        let space_vars = spaces.entry(space.into()).or_default();
        space_vars.insert(key.into(), value.into());
    }

    pub async fn get_space_var(&self, space: &str, key: &str) -> Option<String> {
        self.spaces
            .read()
            .await
            .get(space)
            .and_then(|vars| vars.get(key).cloned())
    }

    pub async fn delete_space_var(&self, space: &str, key: &str) {
        if let Some(vars) = self.spaces.write().await.get_mut(space) {
            vars.remove(key);
        }
    }

    pub async fn list_space_vars(&self, space: &str) -> HashMap<String, String> {
        self.spaces
            .read()
            .await
            .get(space)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn delete_space(&self, space: &str) {
        self.spaces.write().await.remove(space);
    }

    /// Resolves a variable with space variables taking precedence over globals.
    pub async fn resolve(&self, space: Option<&str>, key: &str) -> Option<String> {
        if let Some(space) = space {
            if let Some(value) = self.get_space_var(space, key).await {
                return Some(value);
            }
        }

        self.get_global(key).await
    }
}
