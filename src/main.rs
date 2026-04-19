mod base;
mod manager;
use base::provider::*;
mod config;
use crate::config::CaelixContext;
mod enhancement;
use futures::StreamExt;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 使用CaelixContext初始化
    let context = CaelixContext::new();
    // 初始化提供商配置
    context.init().await.expect("Failed to initialize context");
    
    // 获取提供商
    let provider_manager = context.llm_provider_manager.read().await;
    let provider = provider_manager.get_provider("bailian").expect("Default provider not found");
    println!("Provider initialized successfully");

    // 构建测试消息
    let messages = vec![
        ChatMessage::user("当前目录下都有什么文件,如果有README 则告诉我内容是什么"),
    ];
    
    let agent = context.agent_manager.get("code_executor_agent").await.expect("code_executor_agent 不存在");
    let config = LlmConfig{
        model_name: provider.config().default_model().to_string()
    };
    
    let mut stream = match agent.execute(messages, provider.clone(), &config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ 启动失败: {:?}", e);
            return;
        }
    };
    
    while let Some(item) = stream.next().await {
        match item {
            Ok(text) => {
                // print!("{text}"); 
            }
            Err(e) => {
                eprintln!("错误：{:?}", e);
                break;
            }
        }
    }

    println!("\n\nLLM test completed!");
}