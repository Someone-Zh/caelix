use std::sync::Arc;
use std::io::Write;
use clap::Parser;
use futures::StreamExt;

use crate::api::{CaelixApi, CaelixApiImpl, ChatRequest};
use crate::runtime::message::agent_message::AgentMessageType;
use crate::runtime::message::task_message::TaskMessageType;
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
    
    let cli_args: Vec<String> = if all_args.len() > 1 && (all_args[1] == "cli" || all_args[1].starts_with('-')) {
        // 如果第一个参数是"cli"或是选项，从适当的位置开始
        if all_args.len() > 1 && all_args[1] == "cli" {
            // 如果是 "caelix cli ..." 格式，跳过 "caelix" 和 "cli"
            let mut args: Vec<String> = vec![all_args[0].clone()]; // 保留程序名
            args.extend_from_slice(&all_args[2..]);
            args
        } else {
            // 如果是 "caelix -a ..." 格式，跳过 "caelix" 但保留程序名
            let mut args: Vec<String> = vec![all_args[0].clone()]; // 保留程序名
            args.extend_from_slice(&all_args[1..]);
            args
        }
    } else {
        // 其他情况，只跳过程序名
        let mut args: Vec<String> = vec![all_args[0].clone()]; // 保留程序名
        args.extend_from_slice(&all_args[1..]);
        args
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
    
    let provider_specified = args.provider.is_some();
    let model_specified = args.model.is_some();
    
    let provider = args.provider.unwrap_or_else(|| {
        println!("✅ 使用默认 Provider: {}", default_provider);
        default_provider
    });
    
    if provider_specified {
        println!("✅ 使用指定 Provider: {}", provider);
    }
    
    let model = args.model.unwrap_or_else(|| {
        println!("✅ 使用默认 Model: {}", default_model);
        default_model
    });
    
    if model_specified {
        println!("✅ 使用指定 Model: {}", model);
    }

    println!("\n💡 提示: 输入空行提交消息,输入 /quit 退出\n");

    // 启动后台任务监听任务和通知消息
    let session_id_clone = session_id.clone();
    let message_bus = api.message_bus().clone();
    
    // 监听任务消息
    tokio::spawn(async move {
        let mut task_receiver = message_bus.subscribe_task();
        while let Ok(task_msg) = task_receiver.recv().await {
            if task_msg.session_id == session_id_clone {
                let timestamp = task_msg.timestamp.format("%H:%M:%S");
                let status_icon = match task_msg.r#type {
                    TaskMessageType::Started => "🚀",
                    TaskMessageType::Completed => "✅",
                    TaskMessageType::Failed => "❌",
                    TaskMessageType::Progress => "⏳",
                };
                println!("\n[{}] {} [任务] {}", timestamp, status_icon, task_msg.content);
                let _ = std::io::stdout().flush();
            }
        }
    });
    
    // 监听通知消息
    let session_id_clone2 = session_id.clone();
    let message_bus2 = api.message_bus().clone();
    tokio::spawn(async move {
        let mut notif_receiver = message_bus2.subscribe_notification();
        while let Ok(notif_msg) = notif_receiver.recv().await {
            if notif_msg.session_id == session_id_clone2 {
                let timestamp = notif_msg.timestamp.format("%H:%M:%S");
                println!("\n[{}] 🔔 [通知] {}", timestamp, notif_msg.content);
                let _ = std::io::stdout().flush();
            }
        }
    });

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

        let input_clone = input.clone();
        let request = ChatRequest {
            session_id: session_id.clone(),
            message: input,
            provider: Some(provider.clone()),
            model: Some(model.clone()),
            agent: selected_agent.clone(),
        };

        // 使用新的异步接口
        match api.chat_stream_async(request).await {
            Ok(result) => {
                println!("📡 任务已提交，request_id: {}, span_id: {}", result.request_id, result.span_id);
                
                // 订阅消息流
                match api.subscribe_chat_stream(&result.session_id).await {
                    Ok(mut stream) => {
                        let target_span_id = result.span_id.clone();
                        let mut received_end = false;
                        
                        while let Some(msg) = stream.next().await {
                            match msg.r#type {
                                AgentMessageType::Chunk => {
                                    // 只打印当前 request 和 span_id 的 chunk
                                    if msg.request_id == result.request_id && msg.span_id == target_span_id {
                                        // Chunk 是流式输出，直接打印内容，不加时间戳和换行
                                        print!("{}", msg.content);
                                        let _ = std::io::stdout().flush();
                                    }
                                }
                                AgentMessageType::ChunkEnd => {
                                    // ✅ 关键修改：检查是否是目标 span_id 的结束标记
                                    if msg.span_id == target_span_id {
                                        println!(); // 流结束后换行
                                        received_end = true;
                                        break; // 退出循环，重新开始接收用户输入
                                    }
                                }
                                AgentMessageType::Msg => {
                                    // 显示所有完整消息（包括异步任务的结果）
                                    if msg.content != input_clone {
                                        let timestamp = msg.timestamp.format("%H:%M:%S");
                                        // 尝试解析为 ChatMessage 以获取更友好的显示
                                        if let Ok(chat_msg) = serde_json::from_str::<crate::base::provider::ChatMessage>(&msg.content) {
                                            println!("\n[{}] 💬 [{}] {}", 
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                chat_msg.content);
                                        } else {
                                            println!("\n[{}] 💬 [{}] {}", 
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                msg.content);
                                        }
                                    }
                                }
                            }
                        }
                        
                        if !received_end {
                            println!("\n⚠️  未收到结束信号，可能需要手动中断");
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ 订阅失败: {:?}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ 提交任务失败: {:?}", e);
            }
        }

        println!(); // 添加分隔空行
    }

    Ok(())
}
