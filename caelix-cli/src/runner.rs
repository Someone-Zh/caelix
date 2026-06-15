use clap::Parser;
use futures::StreamExt;
use std::io::Write;
use std::sync::Arc;

use super::commands::handle_command;
use super::input_handler::read_multiline_input;
use caelix_api::message::{AgentMessageType, TaskMessageType};
use caelix_service::{CaelixApi, CaelixApiImpl, ChatRequest};

/// 刷新已完成的 span 缓冲区，按顺序输出
fn flush_completed_spans(
    completed_spans: &mut Vec<String>,
    span_buffers: &mut std::collections::HashMap<String, String>,
    target_span_id: &str,
) {
    // 按完成顺序输出每个 span 的内容
    for span_id in completed_spans.drain(..) {
        if let Some(content) = span_buffers.remove(&span_id) {
            if !content.is_empty() {
                // 如果是目标 span，直接输出（不带标题）
                if span_id == target_span_id {
                    print!("{}", content);
                } else {
                    // 异步任务的 span，带标题输出
                    println!("\n📋 [异步任务结果] {}", content);
                }
                let _ = std::io::stdout().flush();
            }
        }
    }
}

/// 刷新所有剩余的缓冲区（包括活跃和已完成但未输出的）
fn flush_all_buffers(
    active_spans: &mut Vec<String>,
    completed_spans: &mut Vec<String>,
    span_buffers: &mut std::collections::HashMap<String, String>,
    target_span_id: &str,
) {
    // 先输出已完成的
    flush_completed_spans(completed_spans, span_buffers, target_span_id);

    // 再输出活跃的（可能没有收到 ChunkEnd）
    for span_id in active_spans.drain(..) {
        if let Some(content) = span_buffers.remove(&span_id) {
            if !content.is_empty() {
                if span_id == target_span_id {
                    print!("{}", content);
                } else {
                    println!("\n📋 [异步任务结果] {}", content);
                }
                let _ = std::io::stdout().flush();
            }
        }
    }
}

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

    /// 快速对话模式：直接指定消息内容，对话结束后退出
    #[arg(short = 'c', long = "content")]
    content: Option<String>,
}

/// 运行 CLI 后端
pub async fn run_cli(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数，跳过第一个参数（后端名称）
    let all_args: Vec<String> = std::env::args().collect();

    let cli_args: Vec<String> =
        if all_args.len() > 1 && (all_args[1] == "cli" || all_args[1].starts_with('-')) {
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

    // 如果指定了session，先确保会话存在（不存在则创建），然后获取并展示历史对话
    // 先检查会话是否存在，不存在则创建
    if !api.session_exists(&session_id).await {
        println!("ℹ️  会话 {} 不存在，正在创建...", session_id);
        api.create_session_with_id(session_id.clone()).await;
    }

    match api.get_session_messages(&session_id).await {
        Ok(messages) => {
            if !messages.is_empty() {
                println!("\n📜 历史对话 ({} 条消息):", messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    // AgentMessage.content 现在是 ChatMessage 的 JSON 字符串
                    let display_content = if let Ok(chat_msg) =
                        serde_json::from_str::<caelix_api::provider::ChatMessage>(&msg.content)
                    {
                        chat_msg.content
                    } else {
                        msg.content.clone()
                    };

                    let truncated = if display_content.chars().count() > 100 {
                        display_content.chars().take(100).collect::<String>()
                    } else {
                        display_content.clone()
                    };

                    println!(
                        "  [{}] {}: {}",
                        i + 1,
                        msg.timestamp.format("%H:%M:%S"),
                        truncated
                    );
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

    // 如果指定了 -c 参数，进入快速对话模式
    if let Some(content) = args.content {
        println!("🚀 快速对话模式");
        println!("==================");
        println!("💬 用户: {}\n", content);
        println!("🤖 AI 正在回复...\n");

        let request = ChatRequest {
            session_id: session_id.clone(),
            message: Some(content),
            provider: Some(provider.clone()),
            model: Some(model.clone()),
            agent: selected_agent.clone(),
        };

        // 使用新的异步接口
        match api.chat_stream_async(request).await {
            Ok(result) => {
                println!(
                    "📡 任务已提交，request_id: {}, span_id: {}",
                    result.request_id, result.span_id
                );

                // 订阅消息流
                match api.subscribe_chat_stream(&result.session_id).await {
                    Ok(stream) => {
                        let mut stream = stream;
                        let target_span_id = result.span_id.clone();

                        // ✅ 使用缓冲区管理多个 span 的流式输出
                        use std::collections::HashMap;
                        let mut span_buffers: HashMap<String, String> = HashMap::new(); // span_id -> 累积内容
                        let mut active_spans: Vec<String> = Vec::new(); // 保持顺序的活跃 span 列表
                        let mut completed_spans: Vec<String> = Vec::new(); // 已完成的 span 列表
                        let mut target_span_completed = false; // 标记目标 span 是否已完成

                        while let Some(msg) = stream.next().await {
                            match msg.r#type {
                                AgentMessageType::Chunk => {
                                    // 只处理当前会话的消息
                                    if msg.session_id == result.session_id {
                                        let span_id = msg.span_id.clone();

                                        // 如果是新的 span，加入活跃列表
                                        if !span_buffers.contains_key(&span_id) {
                                            span_buffers.insert(span_id.clone(), String::new());
                                            active_spans.push(span_id.clone());
                                        }

                                        // 累积 chunk 内容（不包含 [思考] 等标签）
                                        if let Some(buffer) = span_buffers.get_mut(&span_id) {
                                            buffer.push_str(&msg.content);
                                        }
                                    }
                                }
                                AgentMessageType::ChunkEnd => {
                                    // 标记该 span 完成
                                    if msg.session_id == result.session_id {
                                        let span_id = msg.span_id.clone();

                                        // 检查是否是目标 span
                                        if span_id == target_span_id {
                                            target_span_completed = true;
                                        }

                                        // 从活跃列表移到完成列表
                                        if let Some(pos) =
                                            active_spans.iter().position(|s| s == &span_id)
                                        {
                                            active_spans.remove(pos);
                                            completed_spans.push(span_id.clone());

                                            // ✅ 按顺序输出已完成的 span
                                            flush_completed_spans(
                                                &mut completed_spans,
                                                &mut span_buffers,
                                                &target_span_id,
                                            );
                                        }

                                        // ✅ 如果目标 span 已完成且没有活跃的异步任务 span，退出循环
                                        if target_span_completed && active_spans.is_empty() {
                                            println!("\n✅ 快速对话完成，退出中...");

                                            // 等待消息持久化完成
                                            api.session_manager()
                                                .flush_session(&result.session_id)
                                                .await;

                                            break;
                                        }
                                    }
                                }
                                AgentMessageType::Msg => {
                                    // 显示完整消息（包括异步任务的结果）
                                    if msg.span_id != target_span_id {
                                        let timestamp = msg.timestamp.format("%H:%M:%S");
                                        // 尝试解析为 ChatMessage 以获取更友好的显示
                                        if let Ok(chat_msg) = serde_json::from_str::<
                                            caelix_api::provider::ChatMessage,
                                        >(
                                            &msg.content
                                        ) {
                                            println!(
                                                "\n[{}] 💬 [{}] {}",
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                chat_msg.content
                                            );
                                        } else {
                                            println!(
                                                "\n[{}] 💬 [{}] {}",
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                msg.content
                                            );
                                        }
                                        let _ = std::io::stdout().flush();
                                    }
                                }
                                AgentMessageType::Event => {
                                    // 显示触发事件标记
                                    let timestamp = msg.timestamp.format("%H:%M:%S");
                                    println!(
                                        "\n[{}] ⚡ [{}] {}",
                                        timestamp,
                                        msg.agent_name.as_deref().unwrap_or("AI"),
                                        msg.content
                                    );
                                    let _ = std::io::stdout().flush();
                                }
                                AgentMessageType::ManualApproval => {
                                    let timestamp = msg.timestamp.format("%H:%M:%S");
                                    println!("\n[{}] ⚠️ [需要审批] {}", timestamp, msg.content);
                                    let _ = std::io::stdout().flush();
                                }
                            }
                        }

                        // 最后刷新所有剩余的缓冲（如果循环提前退出，这里不会执行）
                        flush_all_buffers(
                            &mut active_spans,
                            &mut completed_spans,
                            &mut span_buffers,
                            &target_span_id,
                        );
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

        return Ok(());
    }

    // 启动后台任务监听任务和通知消息
    let session_id_clone = session_id.clone();
    let message_bus = api.message_bus().clone();

    // 监听任务消息 - 不需要 RuntimeContext，仅用于打印输出
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
                println!(
                    "\n[{}] {} [任务] {}",
                    timestamp, status_icon, task_msg.content
                );
                let _ = std::io::stdout().flush();
            }
        }
    });

    // 监听通知消息 - 不需要 RuntimeContext，仅用于打印输出
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
            message: Some(input),
            provider: Some(provider.clone()),
            model: Some(model.clone()),
            agent: selected_agent.clone(),
        };

        // 使用新的异步接口
        match api.chat_stream_async(request).await {
            Ok(result) => {
                println!(
                    "📡 任务已提交，request_id: {}, span_id: {}",
                    result.request_id, result.span_id
                );

                // 订阅消息流
                match api.subscribe_chat_stream(&result.session_id).await {
                    Ok(stream) => {
                        let mut stream = stream;
                        let target_span_id = result.span_id.clone();

                        // ✅ 使用缓冲区管理多个 span 的流式输出
                        use std::collections::HashMap;
                        let mut span_buffers: HashMap<String, String> = HashMap::new(); // span_id -> 累积内容
                        let mut active_spans: Vec<String> = Vec::new(); // 保持顺序的活跃 span 列表
                        let mut completed_spans: Vec<String> = Vec::new(); // 已完成的 span 列表

                        while let Some(msg) = stream.next().await {
                            match msg.r#type {
                                AgentMessageType::Chunk => {
                                    // 只处理当前会话的消息
                                    if msg.session_id == result.session_id {
                                        let span_id = msg.span_id.clone();

                                        // 如果是新的 span，加入活跃列表
                                        if !span_buffers.contains_key(&span_id) {
                                            span_buffers.insert(span_id.clone(), String::new());
                                            active_spans.push(span_id.clone());
                                        }

                                        // 累积 chunk 内容（不包含 [思考] 等标签）
                                        if let Some(buffer) = span_buffers.get_mut(&span_id) {
                                            buffer.push_str(&msg.content);
                                        }
                                    }
                                }
                                AgentMessageType::ChunkEnd => {
                                    // 标记该 span 完成
                                    if msg.session_id == result.session_id {
                                        let span_id = msg.span_id.clone();

                                        // 从活跃列表移到完成列表
                                        if let Some(pos) =
                                            active_spans.iter().position(|s| s == &span_id)
                                        {
                                            active_spans.remove(pos);
                                            completed_spans.push(span_id.clone());

                                            // ✅ 按顺序输出已完成的 span
                                            flush_completed_spans(
                                                &mut completed_spans,
                                                &mut span_buffers,
                                                &target_span_id,
                                            );
                                        }
                                    }
                                }
                                AgentMessageType::Msg => {
                                    // 显示完整消息（包括异步任务的结果）
                                    if msg.content != input_clone && msg.span_id != target_span_id {
                                        let timestamp = msg.timestamp.format("%H:%M:%S");
                                        // 尝试解析为 ChatMessage 以获取更友好的显示
                                        if let Ok(chat_msg) = serde_json::from_str::<
                                            caelix_api::provider::ChatMessage,
                                        >(
                                            &msg.content
                                        ) {
                                            println!(
                                                "\n[{}] 💬 [{}] {}",
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                chat_msg.content
                                            );
                                        } else {
                                            println!(
                                                "\n[{}] 💬 [{}] {}",
                                                timestamp,
                                                msg.agent_name.as_deref().unwrap_or("AI"),
                                                msg.content
                                            );
                                        }
                                        let _ = std::io::stdout().flush();
                                    }
                                }
                                AgentMessageType::Event => {
                                    // 显示触发事件标记
                                    let timestamp = msg.timestamp.format("%H:%M:%S");
                                    println!(
                                        "\n[{}] ⚡ [{}] {}",
                                        timestamp,
                                        msg.agent_name.as_deref().unwrap_or("AI"),
                                        msg.content
                                    );
                                    let _ = std::io::stdout().flush();
                                }
                                AgentMessageType::ManualApproval => {
                                    let timestamp = msg.timestamp.format("%H:%M:%S");
                                    println!("\n[{}] ⚠️ [需要审批] {}", timestamp, msg.content);
                                    let _ = std::io::stdout().flush();
                                }
                            }
                        }

                        // 最后刷新所有剩余的缓冲
                        flush_all_buffers(
                            &mut active_spans,
                            &mut completed_spans,
                            &mut span_buffers,
                            &target_span_id,
                        );
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
