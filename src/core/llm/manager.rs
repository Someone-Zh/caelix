use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmType {
    OpenAI,
    GLM,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub name: String,
    pub r#type: LlmType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub models: HashMap<String, String>,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct LlmProviderManager {
    providers: HashMap<String, Box<dyn LlmProvider>>,
}

impl LlmProviderManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }
    
    pub fn add_provider(&mut self, config: LlmProviderConfig) -> Result<(), AgentError> {
        let provider: Box<dyn LlmProvider> = match config.r#type {
            LlmType::OpenAI => {
                Box::new(OpenAIProvider::new(
                    config.api_key,
                    config.base_url,
                    config.models,
                ))
            }
            LlmType::GLM => {
                Box::new(GLMProvider::new(
                    config.api_key,
                    config.base_url,
                    config.models,
                ))
            }
        };
        
        self.providers.insert(config.name, provider);
        Ok(())
    }
    
    pub fn get_provider(&self, name: &str) -> Option<&Box<dyn LlmProvider>> {
        self.providers.get(name)
    }
    
    pub fn get_provider_mut(&mut self, name: &str) -> Option<&mut Box<dyn LlmProvider>> {
        self.providers.get_mut(name)
    }
}