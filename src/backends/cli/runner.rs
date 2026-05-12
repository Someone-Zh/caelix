/// CLI主循环运行器

use std::sync::Arc;
use std::io::Write;
use clap::Parser;
use futures::StreamExt;

use crate::api::{CaelixApi, CaelixApiImpl, ChatRequest};
use crate::base::agent::AgentOutputChunk;
use super::input_handler::read_multiline_input;
use super::commands::handle_command;

/// CLI命令行参数
#[derive(Parser, Debug)]
#[command(name = "caelix")]
#[command(about = "Caelix AI Agent CLI 界面", long_about = None)]
struct CliArgs {
    /// 指定会话ID（未提供则自动创建）
    #[arg(short = 's', long = "session")]
    session_id: Option<String>,

    /// 指定使用的 agent（未提供则使用第一个可用）
    #[arg(short = 'a', long = "agent")]
    agent: Option<String>,

    /// 指定提供商（未提供则使用默认）
    #[arg(short = 'p', long = "provider")]
    provider: Option<String>,

    /// 指定模型（未提供则使用默认）
    #[arg(short = 'm', long = "model")]
    model: Option<String>,
}

/// 运行 CLI 后端
pub async fn run_cli(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数，跳过第一个参数（后端名称）
    let all_args: Vec<String> = std::env::args().collect();
    let cli_args = if all_args.len() > 1 && (all_args[1] == "cli" || all_args[1].starts_with('-')) {
        // 如果第一个参数是"cli"或是选项，从适当的位置开始
        if all_args.len() > 1 && all_args[1] == "cli" {
            all_args[2..].to_vec()
        } else {
            all_args[1..].to_vec()
        }
    } else {
        all_args[1..].to_vec()
    };
    
    let args = CliArgs::parse_from(cli_args);

    println!("🚀 Caelix CLI 模式");
    println!("==================");

    // 确定或创建会话ID
    let session_id = if let Some(sid) = args.session_id {
        println!("✅ 使用指定会话: {}", sid);
        sid
    } else {
        let new_session = api.create_session().await;
        println!("✅ 创建新会话: {}", new_session);
        new_session
    };

    // 如果指定了session，获取并展示历史对话
    match api.get_session_messages(&session_id).await {
        Ok(messages) => {
            if !messages.is_empty() {
                println!("\n📜 历史对话 ({} 条消息):", messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    // AgentMessage.content 现在是 ChatMessage 的 JSON 字符串
                    let display_content = if let Ok(chat_msg) = serde_json::from_str::<crate::base::provider::ChatMessage>(&msg.content) {
                        chat_msg.content
                    } else {
                        msg.content.clone()
                    };
                    
                    println!("  [{}] {}: {}", i + 1, msg.timestamp.format("%H:%M:%S"), 
                             if display_content.len() > 100 { 
                                 &display_content[..100] 
                             } else { 
                                 &display_content 
                             });
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("⚠️  获取历史消息失败: {:?}", e);
        }
    }

    // 获取可用的 agents
    let agents = api.list_agents().await;
    let selected_agent = if let Some(agent_name) = args.agent {
        if agents.contains(&agent_name) {
            println!("✅ 使用指定 Agent: {}", agent_name);
            Some(agent_name)
        } else {
            eprintln!("⚠️  指定的 Agent '{}' 不存在，将使用默认", agent_name);
            None
        }
    } else if !agents.is_empty() {
        println!("📋 可用 Agents: {}", agents.join(", "));
        println!("✅ 使用默认 Agent: {}", agents[0]);
        Some(agents[0].clone())
    } else {
        eprintln!("⚠️  没有可用的 Agents");
        None
    };

    // 显示配置信息
    let default_provider = api.get_default_provider();
    let default_model = api.get_default_model();
    
    let provider = args.provider.unwrap_or_else(|| {
        println!("✅ 使用默认 Provider: {}", default_provider);
        default_provider
    });
    
    let model = args.model.unwrap_or_else(|| {
        println!("✅ 使用默认 Model: {}", default_model);
        default_model
    });

    println!("\n💡 提示: 按 Ctrl+D 结束多行输入,输入 /quit 退出\n");

    // 主循环
    loop {
        // 读取用户输入
        let input = match read_multiline_input() {
            Ok(Some(text)) => text,
            Ok(None) => {
                // Ctrl+D 或 EOF
                println!("\n👋 再见！");
                break;
            }
            Err(e) => {
                eprintln!("❌ 读取输入错误: {}", e);
                continue;
            }
        };

        // 检查是否是命令
        if handle_command(&input) {
            break;
        }

        // 跳过空输入
        if input.trim().is_empty() {
            continue;
        }

        // 发送消息
        println!("\n🤖 AI 正在回复...\n");

        let request = ChatRequest {
            session_id: session_id.clone(),
            message: input,
            provider: Some(provider.clone()),
            model: Some(model.clone()),
            agent: selected_agent.clone(),
        };

        // 调用 chat_stream（RuntimeContext 在 API 层内部管理）
        match api.chat_stream(request).await {
            Ok(mut stream) => {
                // 处理流式响应
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            match chunk {
                                AgentOutputChunk::Content { content } => {
                                    print!("{}", content);
                                    let _ = std::io::stdout().flush();
                                }
                                AgentOutputChunk::ToolCall { name, arguments, .. } => {
                                    println!("\n🔧 调用工具: {}({})", name, arguments);
                                }
                                AgentOutputChunk::Finish { .. } => {
                                    println!();
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            eprintln!("\n❌ 流式响应错误: {:?}", e);
                            break;
                        }
                    }
                }
                println!(); // 添加换行
            }
            Err(e) => {
                eprintln!("❌ 聊天错误: {:?}", e);
            }
        }

        println!(); // 添加分隔空行
    }

    Ok(())
}
