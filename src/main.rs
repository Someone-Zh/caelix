mod base;
use base::provider::*;
use base::tool::DiffEditTool;
use tokio_stream::StreamExt;
use std::sync::Arc;
mod config;
use crate::config::CaelixContext;

#[tokio::main]
async fn main() {
    // 使用CaelixContext初始化
    let context = CaelixContext::new();
    
    // 初始化提供商配置
    context.init_provider().await.expect("Failed to initialize provider");
    
    // 获取提供商
    let provider_manager = context.llm_provider_manager.read().await;
    let provider = provider_manager.get_provider("default").expect("Default provider not found");
    
    println!("Initializing LLM provider...");
    println!("Provider initialized successfully");

    // 构建测试消息
    let messages = vec![
        ChatMessage::user("今天天气怎么样"),
    ];

    // 构建LLM配置
    let llm_config = LlmConfig {
        temperature: 0.7,
        max_tokens: Some(1000),
        model_name: "gpt-3.5-turbo".to_string(), // 使用默认模型名，实际会从配置加载
    };
    
    // Register tools with the context's tool manager
    context.tool_manager.register(Arc::new(DiffEditTool)).await;
    let tools = context.tool_manager.list().await.into_iter().map(|tool| tool.to_definition()).collect::<Vec<_>>();

    // 调用流式接口
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