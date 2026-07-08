use std::sync::Arc;

use caelix_api::variables::VariableManager;
use regex::Regex;

const MAX_RECURSION_DEPTH: u32 = 10;

pub struct VariableReplacer {
    manager: Arc<VariableManager>,
    pattern: Regex,
}

impl VariableReplacer {
    pub fn new(manager: Arc<VariableManager>) -> Self {
        Self {
            manager,
            pattern: Regex::new(r"\{\{([^}]+)\}\}").expect("valid variable regex"),
        }
    }

    pub async fn replace_async(&self, text: &str, space: Option<&str>) -> String {
        let mut result = text.to_string();
        let mut depth = 0;

        while depth < MAX_RECURSION_DEPTH {
            let mut changed = false;

            for capture in self.pattern.captures_iter(&result.clone()) {
                let Some(full_match) = capture.get(0).map(|m| m.as_str()) else {
                    continue;
                };
                let Some(key) = capture.get(1).map(|m| m.as_str().trim()) else {
                    continue;
                };

                if let Some(value) = self.manager.resolve(space, key).await {
                    if result.contains(full_match) {
                        result = result.replace(full_match, &value);
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
            depth += 1;
        }

        result
    }

    pub async fn replace_message(&self, message: &mut String, space: Option<&str>) {
        *message = self.replace_async(message, space).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replaces_global_variables() {
        let manager = Arc::new(VariableManager::new());
        manager.set_global("name", "Alice").await;

        let replacer = VariableReplacer::new(manager);
        let result = replacer.replace_async("Hello {{name}}", None).await;

        assert_eq!(result, "Hello Alice");
    }

    #[tokio::test]
    async fn space_variables_override_globals() {
        let manager = Arc::new(VariableManager::new());
        manager.set_global("project", "Global").await;
        manager
            .set_space_var("workspace", "project", "Scoped")
            .await;

        let replacer = VariableReplacer::new(manager);
        let result = replacer
            .replace_async("Project: {{ project }}", Some("workspace"))
            .await;

        assert_eq!(result, "Project: Scoped");
    }

    #[tokio::test]
    async fn keeps_unknown_variables_unchanged() {
        let manager = Arc::new(VariableManager::new());
        let replacer = VariableReplacer::new(manager);

        let result = replacer.replace_async("Keep {{missing}}", None).await;

        assert_eq!(result, "Keep {{missing}}");
    }

    #[tokio::test]
    async fn recursive_variable_replacement() {
        let manager = Arc::new(VariableManager::new());
        manager.set_global("greeting", "Hello {{name}}").await;
        manager.set_global("name", "Alice").await;

        let replacer = VariableReplacer::new(manager);
        let result = replacer.replace_async("{{greeting}}!", None).await;

        assert_eq!(result, "Hello Alice!");
    }

    #[tokio::test]
    async fn recursion_depth_limit() {
        let manager = Arc::new(VariableManager::new());
        manager.set_global("a", "{{b}}").await;
        manager.set_global("b", "{{a}}").await;

        let replacer = VariableReplacer::new(manager);
        let result = replacer.replace_async("{{a}}", None).await;

        // Should not infinite loop, and should stop at depth limit
        // The exact output depends on how many levels get resolved before hitting limit
        assert!(result.contains("{{a}}") || result.contains("{{b}}"));
    }
}
