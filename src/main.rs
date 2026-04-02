mod base;
use base::llm::*;
use std::env;

#[tokio::main]
async fn main() {
    // 从环境变量中读取配置
    let api_key = env::var("LLM_API_KEY").expect("LLM_API_KEY environment variable is required");
    let base_url = env::var("LLM_API_BASE").ok();
    let model_name = env::var("LLM_API_MODEL").expect("LLM_API_MODEL environment variable is required");

    println!("Initializing LLM provider...");
    println!("Base URL: {:?}", base_url);
    println!("Model: {}", model_name);

    // 创建LLM提供者配置
    let config = LlmProviderConfig {
        name: "openai".to_string(),
        r#type: LlmType::OpenAI,
        api_key,
        base_url,
        models: std::collections::HashMap::from([
            ("default".to_string(), model_name.clone()),
        ]),
        options: serde_json::Value::Null,
    };

    // 初始化LLM提供者管理器
    let mut manager = LlmProviderManager::new();
    manager.add_provider(config).expect("Failed to add LLM provider");

    println!("LLM provider initialized successfully!");

    // 测试LLM调用
    println!("\nTesting LLM call...");
    let provider = manager.get_provider("openai").expect("Failed to get LLM provider");

    // 构建测试消息
    let messages = vec![
        Message {
            role: MessageRole::User,
            content: "Hello, what's your name?".to_string(),
        }
    ];

    // 构建LLM配置
    let llm_config = LlmConfig {
        temperature: 0.7,
        max_tokens: Some(100),
        model_name: "default".to_string(),
    };

    // 调用LLM
    match provider.chat(messages, llm_config).await {
        Ok(response) => {
            println!("Response ID: {}", response.id);
            if let Some(content) = response.content {
                println!("Response content: {}", content);
            } else {
                println!("No content in response");
            }
        }
        Err(e) => {
            println!("Error calling LLM: {:?}", e);
        }
    }

    println!("\nLLM test completed!");
}