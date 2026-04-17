mod base;
mod manager;
use base::provider::*;
use base::tool::DiffEditTool;
use tokio_stream::StreamExt;
use std::sync::Arc;
mod config;
use crate::config::CaelixContext;
mod enhancement;

#[tokio::main]
async fn main() {
    // 使用CaelixContext初始化
    let context = CaelixContext::new();
    // 初始化提供商配置
    context.init().await.expect("Failed to initialize context");
    
    // 获取提供商
    let provider_manager = context.llm_provider_manager.read().await;
    let provider = provider_manager.get_provider("default").expect("Default provider not found");
    
    println!("Provider initialized successfully");

    // 构建测试消息
    let messages = vec![
        ChatMessage::user("当前目录下都有什么文件"),
    ];
    let agent = context.agent_manager.get("code_executor_agent").await.expect("code_executor_agent 不存在");
    
    agent.execute(messages, provider);

    println!("\n\nLLM test completed!");
}