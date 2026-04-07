mod base;
use base::llm::*;
use base::tool::DiffEditTool;
use std::env;
use tokio_stream::StreamExt;
use std::sync::Arc;

use crate::base::tool::manager::TOOL_MANAGER;

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
        llm_type: LlmType::OpenAI,
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
        ChatMessage::user("今天天气怎么样"),
    ];

    // 构建LLM配置
    let llm_config = LlmConfig {
        temperature: 0.7,
        max_tokens: Some(1000),
        model_name: model_name,
    };
    
    TOOL_MANAGER.register(Arc::new(DiffEditTool)).await;
    let tools = TOOL_MANAGER.list().await.into_iter().map(|tool| tool.to_definition()).collect::<Vec<_>>();

    // 调用流式接口（修复所有参数/语法/异步错误）
    let mut stream = provider.chat_stream(&messages, &tools, llm_config).await.expect("Failed to start chat stream");

    // 遍历并打印流式响应
    println!("AI 回复：");
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                // 打印文本内容
                if let Some(content) = chunk.content {
                    print!("{}", content);
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                }
                // 打印工具调用
                if let Some(tool_calls) = chunk.tool_calls {
                    println!("\n[工具调用] {:?}", tool_calls);
                }
            }
            Err(e) => {
                eprintln!("\n流式响应错误: {}", e);
            }
        }
    }

    println!("\n\nLLM test completed!");
}