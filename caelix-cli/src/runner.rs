use clap::Parser;
use futures::StreamExt;
use std::io::Write;
use std::sync::Arc;

use crate::commands::CaelixCommand;
use caelix_api::message::AgentMessageType;
use caelix_service::{CaelixApi, CaelixApiImpl, ChatRequest};

#[derive(Parser, Debug)]
#[command(name = "caelix", version, about = "Caelix AI Agent 命令行工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<CaelixCommand>,
}

fn flush_completed_spans(
    completed_spans: &mut Vec<String>,
    span_buffers: &mut std::collections::HashMap<String, String>,
    target_span_id: &str,
) {
    for span_id in completed_spans.drain(..) {
        if let Some(content) = span_buffers.remove(&span_id)
            && !content.is_empty()
        {
            if span_id == target_span_id {
                print!("{}", content);
            } else {
                println!("\n📋 [异步任务结果] {}", content);
            }
            let _ = std::io::stdout().flush();
        }
    }
}

fn flush_all_buffers(
    active_spans: &mut Vec<String>,
    completed_spans: &mut Vec<String>,
    span_buffers: &mut std::collections::HashMap<String, String>,
    target_span_id: &str,
) {
    flush_completed_spans(completed_spans, span_buffers, target_span_id);

    for span_id in active_spans.drain(..) {
        if let Some(content) = span_buffers.remove(&span_id)
            && !content.is_empty()
        {
            if span_id == target_span_id {
                print!("{}", content);
            } else {
                println!("\n📋 [异步任务结果] {}", content);
            }
            let _ = std::io::stdout().flush();
        }
    }
}

pub async fn run_cli(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    let all_args: Vec<String> = std::env::args().collect();

    let cli_args: Vec<String> = if all_args.len() > 1
        && (all_args[1] == "cli"
            || all_args[1].starts_with('-')
            || all_args[1] == "--help"
            || all_args[1] == "-h")
    {
        if all_args.len() > 1 && all_args[1] == "cli" {
            let mut args = vec![all_args[0].clone()];
            args.extend_from_slice(&all_args[2..]);
            args
        } else {
            let mut args = vec![all_args[0].clone()];
            args.extend_from_slice(&all_args[1..]);
            args
        }
    } else {
        let mut args = vec![all_args[0].clone()];
        args.extend_from_slice(&all_args[1..]);
        args
    };

    let cli = match Cli::try_parse_from(cli_args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };

    match cli.command {
        None => {
            println!("{}", crate::doc::CLI_HELP);
        }
        Some(cmd) => {
            execute_command(cmd, api).await?;
        }
    }

    Ok(())
}

async fn execute_command(
    cmd: CaelixCommand,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        CaelixCommand::Chat(args) => handle_chat(args, api).await,
        CaelixCommand::Tool(args) => handle_tool(args, api).await,
        CaelixCommand::List(args) => handle_list(args, api).await,
        CaelixCommand::Session(args) => handle_session(args, api).await,
        CaelixCommand::Variable(args) => handle_variable(args, api).await,
        CaelixCommand::Agent(args) => handle_agent(args, api).await,
        CaelixCommand::Skill(args) => handle_skill(args, api).await,
        CaelixCommand::Command(args) => handle_cmd(args, api).await,
        CaelixCommand::Hook(args) => handle_hook(args, api).await,
        CaelixCommand::Plugin(args) => handle_plugin(args, api).await,
        CaelixCommand::Security(args) => handle_security(args, api).await,
        CaelixCommand::Provider(args) => handle_provider(args, api).await,
        CaelixCommand::Usage(args) => handle_usage(args, api).await,
        CaelixCommand::Task(args) => handle_task(args, api).await,
        CaelixCommand::Memory(args) => handle_memory(args, api).await,
    }
}

async fn handle_chat(
    args: crate::commands::ChatArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = match args.content {
        Some(c) => c,
        None => {
            eprintln!("❌ 请提供对话内容: -c <内容>");
            println!("\n{}", crate::doc::CHAT_HELP);
            std::process::exit(1);
        }
    };

    let work_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));

    let session_id = if let Some(sid) = args.session_id {
        if !api.session_exists(&sid).await? {
            api.create_session_with_id(sid.clone()).await?;
        }
        sid
    } else {
        api.create_session().await?
    };

    let provider = args.provider.or_else(|| api.get_default_provider());
    let model = args.model.or_else(|| api.get_default_model());
    let agent = args.agent;

    println!("💬 用户: {}\n", content);
    println!("🤖 AI 正在回复...\n");

    let request = ChatRequest {
        session_id: session_id.clone(),
        message: Some(content),
        provider,
        model,
        agent,
        work_dir,
    };

    match api.chat_stream_async(request).await {
        Ok(result) => {
            match api.subscribe_chat_stream(&result.session_id).await {
                Ok(stream) => {
                    let mut stream = stream;
                    let target_span_id = result.span_id.clone();

                    use std::collections::HashMap;
                    let mut span_buffers: HashMap<String, String> = HashMap::new();
                    let mut active_spans: Vec<String> = Vec::new();
                    let mut completed_spans: Vec<String> = Vec::new();
                    let mut target_span_completed = false;

                    while let Some(msg) = stream.next().await {
                        match msg.r#type {
                            AgentMessageType::Chunk => {
                                if msg.session_id == result.session_id {
                                    let span_id = msg.span_id.clone();
                                    if !span_buffers.contains_key(&span_id) {
                                        span_buffers.insert(span_id.clone(), String::new());
                                        active_spans.push(span_id.clone());
                                    }
                                    if let Some(buffer) = span_buffers.get_mut(&span_id) {
                                        buffer.push_str(&msg.content);
                                    }
                                }
                            }
                            AgentMessageType::ChunkEnd => {
                                if msg.session_id == result.session_id {
                                    let span_id = msg.span_id.clone();
                                    if span_id == target_span_id {
                                        target_span_completed = true;
                                    }
                                    if let Some(pos) =
                                        active_spans.iter().position(|s| s == &span_id)
                                    {
                                        active_spans.remove(pos);
                                        completed_spans.push(span_id.clone());
                                        flush_completed_spans(
                                            &mut completed_spans,
                                            &mut span_buffers,
                                            &target_span_id,
                                        );
                                    }
                                    if target_span_completed && active_spans.is_empty() {
                                        println!();
                                        api.session_manager()
                                            .wait_for_session_persistence(&result.session_id)
                                            .await;
                                        break;
                                    }
                                }
                            }
                            AgentMessageType::Msg => {
                                if msg.span_id != target_span_id {
                                    let timestamp = msg.timestamp.format("%H:%M:%S");
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

                    flush_all_buffers(
                        &mut active_spans,
                        &mut completed_spans,
                        &mut span_buffers,
                        &target_span_id,
                    );
                }
                Err(e) => eprintln!("❌ 订阅失败: {:?}", e),
            }
        }
        Err(e) => eprintln!("❌ 提交任务失败: {:?}", e),
    }

    Ok(())
}

async fn handle_tool(
    args: crate::commands::ToolArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None | Some(crate::commands::ToolAction::List) => {
            match api.list_tools().await {
                Ok(tools) => {
                    println!("\n🔧 工具列表 ({} 个):", tools.len());
                    for tool in tools {
                        println!("  {:<30} {}", tool.name, tool.description);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取工具列表失败: {:?}", e),
            }
        }
        Some(crate::commands::ToolAction::Info { name }) => {
            match api.get_tool_info(&name).await {
                Ok(Some(tool)) => {
                    println!("\n📋 工具 {}:", tool.name);
                    println!("  描述: {}", tool.description);
                    println!();
                }
                Ok(None) => println!("\nℹ️  工具 {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取工具信息失败: {:?}", e),
            }
        }
        Some(crate::commands::ToolAction::Exec {
            tool_name,
            args_json,
            extra,
        }) => {
            let arguments = if let Some(json) = args_json {
                serde_json::from_str(&json)
                    .map_err(|e| format!("JSON 参数解析失败: {}", e))?
            } else if !extra.is_empty() {
                let mut map = serde_json::Map::new();
                let mut i = 0;
                while i < extra.len() {
                    if extra[i].starts_with("--") && i + 1 < extra.len() {
                        let key = extra[i].trim_start_matches("--").to_string();
                        let value = extra[i + 1].clone();
                        map.insert(key, serde_json::Value::String(value));
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                serde_json::Value::Object(map)
            } else {
                serde_json::Value::Object(serde_json::Map::new())
            };

            match api.execute_tool(&tool_name, arguments).await {
                Ok(result) => {
                    if let Some(err) = result.error {
                        eprintln!("❌ 工具执行错误: {}", err);
                    } else {
                        println!("{}", result.output);
                    }
                }
                Err(e) => eprintln!("❌ 工具执行失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_list(
    args: crate::commands::ListArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    let list_type = match args.list_type {
        Some(t) => t,
        None => {
            println!("\n{}", crate::doc::LIST_HELP);
            return Ok(());
        }
    };

    match list_type.as_str() {
        "sessions" => {
            match api.list_sessions().await {
                Ok(sessions) => {
                    println!("\n📜 会话列表 ({} 个):", sessions.len());
                    for s in sessions {
                        println!("  {:<25} {} ({})", s.session_id, s.summary, s.created_at);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取会话列表失败: {:?}", e),
            }
        }
        "agents" => {
            match api.list_agents_info().await {
                Ok(agents) => {
                    println!("\n🤖 智能体列表 ({} 个):", agents.len());
                    for agent in agents {
                        print!("  {}", agent.name);
                        if let Some(group) = &agent.group {
                            print!(" [{}]", group);
                        }
                        if !agent.tools.is_empty() {
                            print!(" ({})", agent.tools.join(", "));
                        }
                        println!();
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取智能体列表失败: {:?}", e),
            }
        }
        "tools" => {
            handle_tool(
                crate::commands::ToolArgs {
                    action: Some(crate::commands::ToolAction::List),
                    help: None,
                },
                api,
            )
            .await?;
        }
        "skills" => {
            match api.list_skills().await {
                Ok(skills) => {
                    println!("\n📚 技能列表 ({} 个):", skills.len());
                    for skill in skills {
                        println!("  {:<30} {}", skill.full_name, skill.description);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取技能列表失败: {:?}", e),
            }
        }
        "commands" => {
            match api.list_commands().await {
                Ok(commands) => {
                    println!("\n⚙️  命令列表 ({} 个):", commands.len());
                    for cmd in commands {
                        println!("  {:<25} [{}] {}", cmd.name, cmd.command_type, cmd.description);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取命令列表失败: {:?}", e),
            }
        }
        "hooks" => {
            match api.list_hooks().await {
                Ok(hooks) => {
                    println!("\n🔗 Hook 列表 ({} 个):", hooks.len());
                    for hook in hooks {
                        println!("  {:<25} 能力: {}", hook.name, hook.capabilities);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取 Hook 列表失败: {:?}", e),
            }
        }
        "plugins" => {
            match api.list_plugins().await {
                Ok(plugins) => {
                    println!("\n🧩 插件列表 ({} 个):", plugins.len());
                    for plugin in plugins {
                        println!("  {:<25} 能力: {}", plugin.name, plugin.capabilities);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取插件列表失败: {:?}", e),
            }
        }
        "providers" => {
            match api.get_providers().await {
                Ok(providers) => {
                    println!("\n🌐 提供商列表 ({} 个):", providers.len());
                    for p in providers {
                        println!("  {:<20} ({}) 模型: {}", p.name, p.llm_type, p.models.join(", "));
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取提供商列表失败: {:?}", e),
            }
        }
        other => {
            eprintln!("❌ 未知的列表类型: {}", other);
            println!("\n{}", crate::doc::LIST_HELP);
        }
    }
    Ok(())
}

async fn handle_session(
    args: crate::commands::SessionArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::SESSION_HELP);
            return Ok(());
        }
        Some(crate::commands::SessionAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("sessions".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::SessionAction::Info { session_id }) => {
            match api.get_session_messages(&session_id).await {
                Ok(messages) => {
                    println!("\n📋 会话 {} 消息 ({} 条):", session_id, messages.len());
                    for (i, msg) in messages.iter().enumerate() {
                        let display_content = if let Ok(chat_msg) =
                            serde_json::from_str::<caelix_api::provider::ChatMessage>(&msg.content)
                        {
                            chat_msg.content
                        } else {
                            msg.content.clone()
                        };
                        let truncated = if display_content.chars().count() > 200 {
                            display_content.chars().take(200).collect::<String>() + "..."
                        } else {
                            display_content
                        };
                        println!(
                            "  [{}] {}: {}",
                            i + 1,
                            msg.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            truncated
                        );
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取会话消息失败: {:?}", e),
            }
        }
        Some(crate::commands::SessionAction::Create { session_id }) => {
            match session_id {
                Some(sid) => {
                    api.create_session_with_id(sid.clone()).await?;
                    println!("✅ 会话已创建: {}", sid);
                }
                None => {
                    let sid = api.create_session().await?;
                    println!("✅ 新会话已创建: {}", sid);
                }
            }
        }
        Some(crate::commands::SessionAction::Delete { session_id }) => {
            println!("⚠️  删除会话功能暂未实现: {}", session_id);
        }
        Some(crate::commands::SessionAction::Stop { session_id }) => {
            match api.stop_agent(&session_id).await {
                Ok(true) => println!("✅ 会话 {} 中的 Agent 已停止", session_id),
                Ok(false) => println!("ℹ️  会话 {} 中没有运行中的 Agent", session_id),
                Err(e) => eprintln!("❌ 停止 Agent 失败: {:?}", e),
            }
        }
        Some(crate::commands::SessionAction::SetProvider { session_id, provider }) => {
            match api.set_session_provider(&session_id, &provider).await {
                Ok(_) => println!("✅ 会话 {} 的提供商已设置为: {}", session_id, provider),
                Err(e) => eprintln!("❌ 设置提供商失败: {:?}", e),
            }
        }
        Some(crate::commands::SessionAction::SetModel { session_id, model }) => {
            match api.set_session_model(&session_id, &model).await {
                Ok(_) => println!("✅ 会话 {} 的模型已设置为: {}", session_id, model),
                Err(e) => eprintln!("❌ 设置模型失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_variable(
    args: crate::commands::VariableArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::VARIABLE_HELP);
            return Ok(());
        }
        Some(crate::commands::VariableAction::List) => {
            match api.list_variables().await {
                Ok(vars) => {
                    println!("\n📦 全局变量 ({} 个):", vars.len());
                    for (k, v) in vars {
                        println!("  {} = {}", k, v);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取变量列表失败: {:?}", e),
            }
        }
        Some(crate::commands::VariableAction::Get { key }) => {
            match api.get_variable(&key).await {
                Ok(Some(v)) => println!("\n📦 {} = {}\n", key, v),
                Ok(None) => println!("\n📦 {} 不存在\n", key),
                Err(e) => eprintln!("❌ 获取变量失败: {:?}", e),
            }
        }
        Some(crate::commands::VariableAction::Set { key, value }) => {
            match api.set_variable(&key, &value).await {
                Ok(_) => println!("\n✅ 变量 {} 设置成功\n", key),
                Err(e) => eprintln!("❌ 设置变量失败: {:?}", e),
            }
        }
        Some(crate::commands::VariableAction::Delete { key }) => {
            match api.delete_variable(&key).await {
                Ok(_) => println!("\n✅ 变量 {} 删除成功\n", key),
                Err(e) => eprintln!("❌ 删除变量失败: {:?}", e),
            }
        }
        Some(crate::commands::VariableAction::Space { space, action }) => match action {
            crate::commands::SpaceVariableAction::List => {
                match api.list_space_variables(&space).await {
                    Ok(vars) => {
                        println!("\n📦 空间 {} 变量 ({} 个):", space, vars.len());
                        for (k, v) in vars {
                            println!("  {} = {}", k, v);
                        }
                        println!();
                    }
                    Err(e) => eprintln!("❌ 获取空间变量失败: {:?}", e),
                }
            }
            crate::commands::SpaceVariableAction::Get { key } => {
                match api.get_space_variable(&space, &key).await {
                    Ok(Some(v)) => println!("\n📦 {}.{} = {}\n", space, key, v),
                    Ok(None) => println!("\n📦 {}.{} 不存在\n", space, key),
                    Err(e) => eprintln!("❌ 获取空间变量失败: {:?}", e),
                }
            }
            crate::commands::SpaceVariableAction::Set { key, value } => {
                match api.set_space_variable(&space, &key, &value).await {
                    Ok(_) => println!("\n✅ 空间 {}.{} 设置成功\n", space, key),
                    Err(e) => eprintln!("❌ 设置空间变量失败: {:?}", e),
                }
            }
            crate::commands::SpaceVariableAction::Delete { key } => {
                match api.delete_space_variable(&space, &key).await {
                    Ok(_) => println!("\n✅ 空间 {}.{} 删除成功\n", space, key),
                    Err(e) => eprintln!("❌ 删除空间变量失败: {:?}", e),
                }
            }
        },
        Some(crate::commands::VariableAction::Replace { text }) => {
            match api.replace_variables(&text, None).await {
                Ok(result) => println!("{}", result),
                Err(e) => eprintln!("❌ 变量替换失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_agent(
    args: crate::commands::AgentArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::AGENT_HELP);
            return Ok(());
        }
        Some(crate::commands::AgentAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("agents".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::AgentAction::Info { name }) => {
            match api.get_agent_info(&name).await {
                Ok(Some(agent)) => {
                    println!("\n📋 智能体 {}:", agent.name);
                    if let Some(group) = &agent.group {
                        println!("  组: {}", group);
                    }
                    println!("  工具: {}", agent.tools.join(", "));
                    println!();
                }
                Ok(None) => println!("\nℹ️  智能体 {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取智能体信息失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_skill(
    args: crate::commands::SkillArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::SKILL_HELP);
            return Ok(());
        }
        Some(crate::commands::SkillAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("skills".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::SkillAction::Info { name }) => {
            match api.get_skill_info(&name).await {
                Ok(Some(skill)) => {
                    println!("\n📋 技能 {}:", skill.full_name);
                    println!("  名称: {}", skill.name);
                    println!("  命名空间: {}", skill.namespace);
                    println!("  描述: {}", skill.description);
                    if let Some(version) = &skill.version {
                        println!("  版本: {}", version);
                    }
                    if let Some(author) = &skill.author {
                        println!("  作者: {}", author);
                    }
                    if !skill.tags.is_empty() {
                        println!("  标签: {}", skill.tags.join(", "));
                    }
                    println!();
                }
                Ok(None) => println!("\nℹ️  技能 {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取技能信息失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_cmd(
    args: crate::commands::CommandArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::COMMAND_HELP);
            return Ok(());
        }
        Some(crate::commands::CommandAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("commands".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::CommandAction::Info { name }) => {
            match api.get_command_info(&name).await {
                Ok(Some(cmd)) => {
                    println!("\n📋 命令 {}:", cmd.name);
                    println!("  类型: {}", cmd.command_type);
                    println!("  描述: {}", cmd.description);
                    println!();
                }
                Ok(None) => println!("\nℹ️  命令 {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取命令信息失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_hook(
    args: crate::commands::HookArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::HOOK_HELP);
            return Ok(());
        }
        Some(crate::commands::HookAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("hooks".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::HookAction::Info { name }) => {
            match api.get_hook_info(&name).await {
                Ok(Some(hook)) => {
                    println!("\n📋 Hook {}:", hook.name);
                    println!("  能力: {:?}", hook.capabilities);
                    println!("  作用范围: {:?}", hook.scope);
                    println!();
                }
                Ok(None) => println!("\nℹ️  Hook {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取 Hook 信息失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_plugin(
    args: crate::commands::PluginArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::PLUGIN_HELP);
            return Ok(());
        }
        Some(crate::commands::PluginAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("plugins".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::PluginAction::Info { name }) => {
            match api.get_plugin_info(&name).await {
                Ok(Some(plugin)) => {
                    println!("\n📋 插件 {}:", plugin.name);
                    println!("  能力: {:?}", plugin.capabilities);
                    println!();
                }
                Ok(None) => println!("\nℹ️  插件 {} 不存在\n", name),
                Err(e) => eprintln!("❌ 获取插件信息失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_security(
    args: crate::commands::SecurityArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::SECURITY_HELP);
            return Ok(());
        }
        Some(crate::commands::SecurityAction::Config) => {
            match api.get_security_config().await {
                Ok(config) => {
                    println!("\n🔒 安全配置:");
                    println!("  路径允许: {:?}", config.config.path.include);
                    println!("  路径排除: {:?}", config.config.path.exclude);
                    println!("  URL 允许: {:?}", config.config.url.include);
                    println!("  URL 排除: {:?}", config.config.url.exclude);
                    println!("  命令允许: {:?}", config.config.command.include);
                    println!("  命令排除: {:?}", config.config.command.exclude);
                    println!();
                }
                Err(e) => eprintln!("❌ 获取安全配置失败: {:?}", e),
            }
        }
        Some(crate::commands::SecurityAction::Check { target }) => match target {
            crate::commands::SecurityCheckTarget::Path { path } => {
                match api.check_path(&path).await {
                    Ok(safe) => println!("\n🔒 路径 {} 安全: {}\n", path, safe),
                    Err(e) => eprintln!("❌ 检查路径失败: {:?}", e),
                }
            }
            crate::commands::SecurityCheckTarget::Url { url } => {
                match api.check_url(&url).await {
                    Ok(safe) => println!("\n🔒 URL {} 安全: {}\n", url, safe),
                    Err(e) => eprintln!("❌ 检查 URL 失败: {:?}", e),
                }
            }
            crate::commands::SecurityCheckTarget::Command { command } => {
                match api.check_command(&command).await {
                    Ok(safe) => println!("\n🔒 命令 {} 安全: {}\n", command, safe),
                    Err(e) => eprintln!("❌ 检查命令失败: {:?}", e),
                }
            }
        },
        Some(crate::commands::SecurityAction::Add { rule }) => {
            let result = match rule {
                crate::commands::SecurityAddRule::Path { action } => match action {
                    crate::commands::IncludeExclude::Include { value } => {
                        api.add_path_include(&value).await
                    }
                    crate::commands::IncludeExclude::Exclude { value } => {
                        api.add_path_exclude(&value).await
                    }
                },
                crate::commands::SecurityAddRule::Url { action } => match action {
                    crate::commands::IncludeExclude::Include { value } => {
                        api.add_url_include(&value).await
                    }
                    crate::commands::IncludeExclude::Exclude { value } => {
                        api.add_url_exclude(&value).await
                    }
                },
                crate::commands::SecurityAddRule::Command { action } => match action {
                    crate::commands::IncludeExclude::Include { value } => {
                        api.add_command_include(&value).await
                    }
                    crate::commands::IncludeExclude::Exclude { value } => {
                        api.add_command_exclude(&value).await
                    }
                },
            };
            match result {
                Ok(_) => println!("\n✅ 安全规则添加成功\n"),
                Err(e) => eprintln!("❌ 添加安全规则失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_provider(
    args: crate::commands::ProviderArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::PROVIDER_HELP);
            return Ok(());
        }
        Some(crate::commands::ProviderAction::List) => {
            handle_list(
                crate::commands::ListArgs {
                    list_type: Some("providers".to_string()),
                    help: None,
                },
                api,
            )
            .await?;
        }
        Some(crate::commands::ProviderAction::Models { name }) => {
            match api.get_provider_models(&name).await {
                Ok(models) => {
                    println!("\n📦 提供商 {} 的模型 ({} 个):", name, models.len());
                    for m in models {
                        println!("  - {}", m);
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取模型列表失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_usage(
    args: crate::commands::UsageArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.global || args.session_id.is_none() {
        match api.get_global_usage().await {
            Ok(view) => {
                println!("\n==================================");
                println!("  🌍 Token 用量总览 (全局)");
                println!("==================================");
                println!("  Prompt Tokens     : {}", view.total.prompt_tokens);
                println!("  Completion Tokens : {}", view.total.completion_tokens);
                println!("  Total Tokens      : {}", view.total.total_tokens);
                if view.total.reasoning_tokens > 0 {
                    println!("  Reasoning Tokens  : {}", view.total.reasoning_tokens);
                }
                if view.total.cache_hit_tokens > 0 {
                    println!("  Cache Hit Tokens  : {}", view.total.cache_hit_tokens);
                }
                println!("  记录数            : {}", view.total.record_count);

                if !view.by_provider_model.is_empty() {
                    println!("\n  按 Provider / Model:");
                    println!("  ------------------------------");
                    for item in &view.by_provider_model {
                        println!(
                            "  {} @ {}: prompt={}, completion={}, total={}",
                            item.provider,
                            item.model,
                            item.snapshot.prompt_tokens,
                            item.snapshot.completion_tokens,
                            item.snapshot.total_tokens,
                        );
                    }
                }

                if !view.by_session.is_empty() {
                    println!("\n  按 Session:");
                    println!("  ------------------------------");
                    for item in &view.by_session {
                        println!(
                            "  {}: context_size={}, total={}",
                            item.session_id, item.context_size_tokens, item.snapshot.total_tokens,
                        );
                    }
                }
                println!("==================================\n");
            }
            Err(e) => eprintln!("❌ 获取全局用量失败: {:?}", e),
        }
    } else if let Some(sid) = args.session_id {
        match api.get_session_usage(&sid).await {
            Ok(Some(view)) => {
                println!("\n==================================");
                println!("  📊 Session Token 用量");
                println!("==================================");
                println!("  Session ID        : {}", view.session_id);
                println!("  Prompt Tokens     : {}", view.snapshot.prompt_tokens);
                println!("  Completion Tokens : {}", view.snapshot.completion_tokens);
                println!("  Total Tokens      : {}", view.snapshot.total_tokens);
                if view.snapshot.reasoning_tokens > 0 {
                    println!("  Reasoning Tokens  : {}", view.snapshot.reasoning_tokens);
                }
                if view.snapshot.cache_hit_tokens > 0 {
                    println!("  Cache Hit Tokens  : {}", view.snapshot.cache_hit_tokens);
                }
                println!("  记录数            : {}", view.snapshot.record_count);
                println!("  上下文大小        : {} tokens", view.context_size_tokens);
                if let Some(limit) = view.ctx_window_tokens {
                    let pct = if limit > 0 {
                        (view.context_size_tokens as f64 / limit as f64) * 100.0
                    } else {
                        0.0
                    };
                    println!("  上下文窗口上限    : {} tokens ({:.1}%)", limit, pct);
                } else {
                    println!("  上下文窗口上限    : 未配置");
                }
                println!("==================================\n");
            }
            Ok(None) => println!("\nℹ️  Session {} 暂无 token 用量记录\n", sid),
            Err(e) => eprintln!("❌ 获取 Session 用量失败: {:?}", e),
        }
    }
    Ok(())
}

async fn handle_task(
    args: crate::commands::TaskArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::TASK_HELP);
            return Ok(());
        }
        Some(crate::commands::TaskAction::List { session_id }) => {
            match api.list_tasks(session_id.as_deref()).await {
                Ok(tasks) => {
                    println!("\n📋 任务列表 ({} 个):", tasks.len());
                    for task in tasks {
                        use caelix_api::task::TaskStatus;
                        let status_icon = match &task.status {
                            TaskStatus::Pending => "⏳",
                            TaskStatus::Scheduled => "📅",
                            TaskStatus::Running => "🚀",
                            TaskStatus::Completed => "✅",
                            TaskStatus::Failed(_) => "❌",
                            TaskStatus::Cancelled => "🚫",
                        };
                        let status_str = match &task.status {
                            TaskStatus::Pending => "pending",
                            TaskStatus::Scheduled => "scheduled",
                            TaskStatus::Running => "running",
                            TaskStatus::Completed => "completed",
                            TaskStatus::Failed(_) => "failed",
                            TaskStatus::Cancelled => "cancelled",
                        };
                        let desc = task.task_name.as_deref().unwrap_or(&task.task_type_name);
                        println!(
                            "  {} {} [{}] {}",
                            status_icon,
                            task.task_id,
                            status_str,
                            desc
                        );
                    }
                    println!();
                }
                Err(e) => eprintln!("❌ 获取任务列表失败: {:?}", e),
            }
        }
    }
    Ok(())
}

async fn handle_memory(
    args: crate::commands::MemoryArgs,
    api: Arc<CaelixApiImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        None => {
            println!("\n{}", crate::doc::MEMORY_HELP);
            return Ok(());
        }
        Some(crate::commands::MemoryAction::Recall { query, top_k }) => {
            match api.memory_recall(&query, top_k).await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("(未找到相关记忆)");
                        return Ok(());
                    }
                    println!("==================================");
                    println!("  📚 记忆检索结果 ({} 条)", results.len());
                    println!("==================================");
                    for (i, result) in results.iter().enumerate() {
                        let layer_color = match result.layer.as_str() {
                            "Axiom" => "🔮",
                            "Wiki" => "📖",
                            "Raw" => "📝",
                            _ => "📄",
                        };
                        let conf = result
                            .confidence
                            .map(|c| format!(" ({:.0}%)", c * 100.0))
                            .unwrap_or_default();
                        println!("\n  [{i}] {layer_color} [{}]{conf}", result.layer);
                        println!("     文件: {}", result.file);
                        println!("     标题: {}", result.heading);
                        println!("     预览: {}", result.preview);
                    }
                    println!("\n==================================");
                }
                Err(e) => eprintln!("❌ 检索失败: {}", e),
            }
        }
        Some(crate::commands::MemoryAction::Write {
            content,
            source,
            tags,
        }) => {
            match api.memory_write(&content, &source, tags).await {
                Ok(_) => println!("✅ 已写入 Raw 层"),
                Err(e) => eprintln!("❌ 写入失败: {}", e),
            }
        }
        Some(crate::commands::MemoryAction::Promote { raw, wiki }) => {
            if let Some(file) = raw {
                match api.memory_promote_raw(&file).await {
                    Ok(_) => println!("🔄 手动触发 Raw→Wiki 晋升: {}", file),
                    Err(e) => eprintln!("❌ 晋升失败: {}", e),
                }
            }
            if let Some(entity) = wiki {
                match api.memory_promote_wiki(&entity).await {
                    Ok(_) => println!("🔄 手动触发 Wiki→Axiom 晋升: {}", entity),
                    Err(e) => eprintln!("❌ 晋升失败: {}", e),
                }
            }
        }
        Some(crate::commands::MemoryAction::Flags { all }) => {
            let conflicts = api.memory_list_conflicts(all).await?;
            let candidates = api.memory_list_candidates(all).await?;

            println!("==================================");
            println!("  ⚠️  冲突与候选列表");
            println!("==================================");

            if !conflicts.is_empty() {
                println!("\n  🚫 冲突 ({})", conflicts.len());
                println!("  ------------------------------");
                for conflict in &conflicts {
                    let status_icon = if conflict.status == "Pending" { "⏳" } else { "✅" };
                    println!(
                        "  {} {} [{}] {} - {}",
                        status_icon,
                        conflict.id,
                        conflict.r#type,
                        conflict.entity,
                        conflict.field.as_deref().unwrap_or("")
                    );
                    for value in &conflict.values {
                        println!("       - {}", value);
                    }
                }
            }

            if !candidates.is_empty() {
                println!("\n  📋 Axiom 候选 ({})", candidates.len());
                println!("  ------------------------------");
                for candidate in &candidates {
                    let status_icon = match candidate.status.as_str() {
                        "Pending" => "⏳",
                        "Approved" => "✅",
                        "Rejected" => "❌",
                        _ => "📄",
                    };
                    println!(
                        "  {} {} (confidence: {:.0}%)",
                        status_icon,
                        candidate.id,
                        candidate.confidence * 100.0
                    );
                    println!("       {}", candidate.preview);
                }
            }

            if conflicts.is_empty() && candidates.is_empty() {
                println!("(暂无冲突或候选)");
            }
            println!("\n==================================");
        }
        Some(crate::commands::MemoryAction::RebuildIndex) => {
            println!("🔄 正在重建反向索引...");
            match api.memory_rebuild_index().await {
                Ok(_) => println!("✅ 索引重建完成"),
                Err(e) => eprintln!("❌ 索引重建失败: {}", e),
            }
        }
        Some(crate::commands::MemoryAction::Stats) => {
            match api.memory_stats().await {
                Ok(stats) => {
                    println!("==================================");
                    println!("  📊 Memory Vault 统计");
                    println!("==================================");
                    println!("  Raw 文件数        : {}", stats.raw_files);
                    println!("  Wiki 实体数       : {}", stats.wiki_entities);
                    println!("  Wiki 事件数       : {}", stats.wiki_events);
                    println!(
                        "  Axiom 总数        : {} (活跃: {})",
                        stats.axioms, stats.axioms_active
                    );
                    println!("  待处理冲突        : {}", stats.pending_conflicts);
                    println!("  Axiom 候选        : {}", stats.pending_candidates);
                    println!("  待创建链接        : {}", stats.pending_links);
                    println!(
                        "  LLM 预算          : {}/{}",
                        stats.llm_budget_used, stats.llm_budget_total
                    );
                    println!("==================================");
                }
                Err(e) => eprintln!("❌ 获取统计失败: {}", e),
            }
        }
        Some(crate::commands::MemoryAction::Axioms { include_deprecated }) => {
            match api.memory_list_axioms(include_deprecated).await {
                Ok(axioms) => {
                    println!("==================================");
                    println!("  🔮 Axiom 列表 ({} 条)", axioms.len());
                    if include_deprecated {
                        println!("  (包含已废弃)");
                    }
                    println!("==================================");
                    for axiom in &axioms {
                        let status_icon = if axiom.status == "Active" { "✅" } else { "❌" };
                        println!("\n  {} {} [{}]", status_icon, axiom.name, axiom.category);
                        println!("     置信度: {:.0}%", axiom.confidence * 100.0);
                        println!("     创建于: {}", axiom.created_at);
                        if let Some(reason) = &axiom.deprecated_reason {
                            println!("     废弃原因: {}", reason);
                        }
                    }
                    if axioms.is_empty() {
                        println!("(暂无 Axiom)");
                    }
                    println!("\n==================================");
                }
                Err(e) => eprintln!("❌ 获取 Axiom 列表失败: {}", e),
            }
        }
        Some(crate::commands::MemoryAction::Budget) => {
            match api.memory_budget().await {
                Ok(info) => {
                    let status_icon = if info.exhausted { "⚠️" } else { "✅" };
                    println!("==================================");
                    println!("  💰 LLM 调用预算");
                    println!("==================================");
                    println!("  {} 今日已用: {}/{}", status_icon, info.used, info.budget);
                    println!("     剩余: {}", info.remaining);
                    if info.exhausted {
                        println!("     ⚠️  预算已耗尽，晋升任务将被延迟");
                    }
                    println!("==================================");
                }
                Err(e) => eprintln!("❌ 获取预算信息失败: {}", e),
            }
        }
    }
    Ok(())
}
