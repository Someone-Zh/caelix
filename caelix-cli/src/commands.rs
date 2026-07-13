use std::sync::Arc;

use caelix_service::{CaelixApi, CaelixApiImpl};

pub fn is_quit_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/quit" || trimmed == "/exit" || trimmed == "/q"
}

pub fn handle_command(input: &str) -> bool {
    if is_quit_command(input) {
        println!("\n👋 再见！");
        return true;
    }
    false
}

pub fn is_cli_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

pub async fn handle_cli_command(input: &str, session_id: &str, api: &Arc<CaelixApiImpl>) {
    let trimmed = input.trim();
    let args: Vec<&str> = trimmed.split_whitespace().collect();
    if args.is_empty() {
        return;
    }

    match args[0] {
        "/help" => show_help(),
        "/quit" | "/exit" | "/q" => (),
        "/usage" => handle_usage_command(trimmed, session_id, api).await,
        "/session" => handle_session_command(trimmed, session_id, api).await,
        "/variable" => handle_variable_command(trimmed, api).await,
        "/agent" => handle_agent_command(trimmed, api).await,
        "/skill" => handle_skill_command(trimmed, api).await,
        "/command" => handle_command_command(trimmed, api).await,
        "/tool" => handle_tool_command(trimmed, api).await,
        "/hook" => handle_hook_command(trimmed, api).await,
        "/plugin" => handle_plugin_command(trimmed, api).await,
        "/security" => handle_security_command(trimmed, api).await,
        "/provider" => handle_provider_command(trimmed, api).await,
        _ => eprintln!("⚠️  未知命令: {}", args[0]),
    }
}

fn show_help() {
    println!("\n📋 Caelix CLI 命令列表:");
    println!("======================");
    println!("  /help              显示此帮助信息");
    println!("  /quit /exit /q     退出 CLI");
    println!("  /usage [--session <id>|--global]  显示 Token 用量");
    println!("  /session list      列出所有会话");
    println!("  /session info <id> 显示会话详情");
    println!("  /session delete <id> 删除会话");
    println!("  /variable list     列出所有全局变量");
    println!("  /variable get <key> 获取变量值");
    println!("  /variable set <key> <value> 设置变量");
    println!("  /variable delete <key> 删除变量");
    println!("  /variable space <space> [list|get|set|delete] <key> [value]");
    println!("  /agent list        列出所有智能体");
    println!("  /agent info <name> 显示智能体详情");
    println!("  /skill list        列出所有技能");
    println!("  /skill info <name> 显示技能详情");
    println!("  /command list      列出所有命令");
    println!("  /command info <name> 显示命令详情");
    println!("  /tool list         列出所有工具");
    println!("  /tool info <name>  显示工具详情");
    println!("  /hook list         列出所有钩子");
    println!("  /hook info <name>  显示钩子详情");
    println!("  /plugin list       列出所有插件");
    println!("  /plugin info <name> 显示插件详情");
    println!("  /security config   显示安全配置");
    println!("  /security check path|url|command <target>");
    println!("  /security add path|url|command include|exclude <value>");
    println!("  /provider list     列出所有提供者");
    println!("  /provider models <name> 显示提供者模型");
    println!("======================\n");
}

pub async fn handle_usage_command(input: &str, session_id: &str, api: &Arc<CaelixApiImpl>) {
    let trimmed = input.trim();
    let args: Vec<&str> = trimmed.split_whitespace().collect();

    let mut target_session: Option<String> = None;
    let mut show_global = false;

    let mut i = 1;
    while i < args.len() {
        match args[i] {
            "--session" | "-s" => {
                if i + 1 < args.len() {
                    target_session = Some(args[i + 1].to_string());
                    i += 2;
                } else {
                    eprintln!("⚠️  --session 参数需要一个值");
                    return;
                }
            }
            "--global" | "-g" => {
                show_global = true;
                i += 1;
            }
            other => {
                eprintln!("⚠️  未知参数: {}", other);
                return;
            }
        }
    }

    if show_global {
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
            Err(e) => {
                eprintln!("⚠️  获取全局用量失败: {:?}", e);
            }
        }
        return;
    }

    let sid = target_session.as_deref().unwrap_or(session_id);
    match api.get_session_usage(sid).await {
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
        Ok(None) => {
            println!("\nℹ️  Session {} 暂无 token 用量记录\n", sid);
        }
        Err(e) => {
            eprintln!("⚠️  获取 Session 用量失败: {:?}", e);
        }
    }
}

async fn handle_session_command(input: &str, _session_id: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /session list|info|delete");
        return;
    }

    match args[1] {
        "list" => match api.list_sessions().await {
            Ok(sessions) => {
                println!("\n📜 会话列表:");
                for s in sessions {
                    println!("  {} - {} ({})", s.session_id, s.summary, s.created_at);
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取会话列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少会话ID，使用 /session info <id>");
                return;
            }
            let sid = args[2];
            match api.get_session_messages(sid).await {
                Ok(messages) => {
                    println!("\n📋 会话 {} 消息 ({} 条):", sid, messages.len());
                    for (i, msg) in messages.iter().enumerate() {
                        println!("  [{}] {}: {}", i + 1, msg.timestamp, msg.content);
                    }
                    println!();
                }
                Err(e) => eprintln!("⚠️  获取会话消息失败: {:?}", e),
            }
        }
        "delete" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少会话ID，使用 /session delete <id>");
                return;
            }
            let sid = args[2];
            println!("⚠️  删除会话功能尚未实现: {}", sid);
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_variable_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /variable list|get|set|delete|space");
        return;
    }

    match args[1] {
        "list" => match api.list_variables().await {
            Ok(vars) => {
                println!("\n📦 全局变量列表:");
                for (k, v) in vars {
                    println!("  {} = {}", k, v);
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取变量列表失败: {:?}", e),
        },
        "get" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少变量名，使用 /variable get <key>");
                return;
            }
            match api.get_variable(args[2]).await {
                Ok(Some(v)) => println!("\n📦 {} = {}\n", args[2], v),
                Ok(None) => println!("\n📦 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取变量失败: {:?}", e),
            }
        }
        "set" => {
            if args.len() < 4 {
                eprintln!("⚠️  参数不足，使用 /variable set <key> <value>");
                return;
            }
            let value = args[3..].join(" ");
            match api.set_variable(args[2], &value).await {
                Ok(_) => println!("\n✅ 变量 {} 设置成功\n", args[2]),
                Err(e) => eprintln!("⚠️  设置变量失败: {:?}", e),
            }
        }
        "delete" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少变量名，使用 /variable delete <key>");
                return;
            }
            match api.delete_variable(args[2]).await {
                Ok(_) => println!("\n✅ 变量 {} 删除成功\n", args[2]),
                Err(e) => eprintln!("⚠️  删除变量失败: {:?}", e),
            }
        }
        "space" => {
            if args.len() < 3 {
                eprintln!("⚠️  参数不足，使用 /variable space <space> [list|get|set|delete]");
                return;
            }
            let space = args[2];
            if args.len() < 4 {
                match api.list_space_variables(space).await {
                    Ok(vars) => {
                        println!("\n📦 空间 {} 变量列表:", space);
                        for (k, v) in vars {
                            println!("  {} = {}", k, v);
                        }
                        println!();
                    }
                    Err(e) => eprintln!("⚠️  获取空间变量失败: {:?}", e),
                }
                return;
            }
            match args[3] {
                "list" => match api.list_space_variables(space).await {
                    Ok(vars) => {
                        println!("\n📦 空间 {} 变量列表:", space);
                        for (k, v) in vars {
                            println!("  {} = {}", k, v);
                        }
                        println!();
                    }
                    Err(e) => eprintln!("⚠️  获取空间变量失败: {:?}", e),
                },
                "get" => {
                    if args.len() < 5 {
                        eprintln!("⚠️  缺少变量名，使用 /variable space <space> get <key>");
                        return;
                    }
                    match api.get_space_variable(space, args[4]).await {
                        Ok(Some(v)) => println!("\n📦 {}.{} = {}\n", space, args[4], v),
                        Ok(None) => println!("\n📦 {}.{} 不存在\n", space, args[4]),
                        Err(e) => eprintln!("⚠️  获取空间变量失败: {:?}", e),
                    }
                }
                "set" => {
                    if args.len() < 6 {
                        eprintln!("⚠️  参数不足，使用 /variable space <space> set <key> <value>");
                        return;
                    }
                    let value = args[5..].join(" ");
                    match api.set_space_variable(space, args[4], &value).await {
                        Ok(_) => println!("\n✅ 空间 {}.{} 设置成功\n", space, args[4]),
                        Err(e) => eprintln!("⚠️  设置空间变量失败: {:?}", e),
                    }
                }
                "delete" => {
                    if args.len() < 5 {
                        eprintln!("⚠️  缺少变量名，使用 /variable space <space> delete <key>");
                        return;
                    }
                    match api.delete_space_variable(space, args[4]).await {
                        Ok(_) => println!("\n✅ 空间 {}.{} 删除成功\n", space, args[4]),
                        Err(e) => eprintln!("⚠️  删除空间变量失败: {:?}", e),
                    }
                }
                _ => eprintln!("⚠️  未知子命令: {}", args[3]),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_agent_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /agent list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_agents_info().await {
            Ok(agents) => {
                println!("\n🤖 智能体列表:");
                for agent in agents {
                    println!("  {}", agent.name);
                    if let Some(group) = &agent.group {
                        println!("    组: {}", group);
                    }
                    if !agent.tools.is_empty() {
                        println!("    工具: {}", agent.tools.join(", "));
                    }
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取智能体列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少智能体名称，使用 /agent info <name>");
                return;
            }
            match api.get_agent_info(args[2]).await {
                Ok(Some(agent)) => {
                    println!("\n📋 智能体 {}:", agent.name);
                    if let Some(group) = &agent.group {
                        println!("  组: {}", group);
                    }
                    println!("  工具: {}", agent.tools.join(", "));
                    println!();
                }
                Ok(None) => println!("\nℹ️  智能体 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取智能体信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_skill_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /skill list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_skills().await {
            Ok(skills) => {
                println!("\n📚 技能列表:");
                for skill in skills {
                    println!("  {} ({}::{})", skill.full_name, skill.namespace, skill.name);
                    if !skill.description.is_empty() {
                        println!("    描述: {}", skill.description);
                    }
                    if let Some(version) = &skill.version {
                        println!("    版本: {}", version);
                    }
                    if !skill.tags.is_empty() {
                        println!("    标签: {}", skill.tags.join(", "));
                    }
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取技能列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少技能名称，使用 /skill info <name>");
                return;
            }
            match api.get_skill_info(args[2]).await {
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
                Ok(None) => println!("\nℹ️  技能 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取技能信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_command_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /command list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_commands().await {
            Ok(commands) => {
                println!("\n⚙️  命令列表:");
                for cmd in commands {
                    println!("  {} [{}]", cmd.name, cmd.command_type);
                    if !cmd.description.is_empty() {
                        println!("    描述: {}", cmd.description);
                    }
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取命令列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少命令名称，使用 /command info <name>");
                return;
            }
            match api.get_command_info(args[2]).await {
                Ok(Some(cmd)) => {
                    println!("\n📋 命令 {}:", cmd.name);
                    println!("  类型: {}", cmd.command_type);
                    println!("  描述: {}", cmd.description);
                    println!();
                }
                Ok(None) => println!("\nℹ️  命令 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取命令信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_tool_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /tool list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_tools().await {
            Ok(tools) => {
                println!("\n🔧 工具列表:");
                for tool in tools {
                    println!("  {}", tool.name);
                    if !tool.description.is_empty() {
                        println!("    描述: {}", tool.description);
                    }
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取工具列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少工具名称，使用 /tool info <name>");
                return;
            }
            match api.get_tool_info(args[2]).await {
                Ok(Some(tool)) => {
                    println!("\n📋 工具 {}:", tool.name);
                    println!("  描述: {}", tool.description);
                    println!();
                }
                Ok(None) => println!("\nℹ️  工具 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取工具信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_hook_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /hook list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_hooks().await {
            Ok(hooks) => {
                println!("\n🔗 钩子列表:");
                for hook in hooks {
                    println!("  {}", hook.name);
                    println!("    能力: {:?}", hook.capabilities);
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取钩子列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少钩子名称，使用 /hook info <name>");
                return;
            }
            match api.get_hook_info(args[2]).await {
                Ok(Some(hook)) => {
                    println!("\n📋 钩子 {}:", hook.name);
                    println!("  能力: {:?}", hook.capabilities);
                    println!("  作用范围: {:?}", hook.scope);
                    println!();
                }
                Ok(None) => println!("\nℹ️  钩子 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取钩子信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_plugin_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /plugin list|info");
        return;
    }

    match args[1] {
        "list" => match api.list_plugins().await {
            Ok(plugins) => {
                println!("\n🧩 插件列表:");
                for plugin in plugins {
                    println!("  {}", plugin.name);
                    println!("    能力: {:?}", plugin.capabilities);
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取插件列表失败: {:?}", e),
        },
        "info" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少插件名称，使用 /plugin info <name>");
                return;
            }
            match api.get_plugin_info(args[2]).await {
                Ok(Some(plugin)) => {
                    println!("\n📋 插件 {}:", plugin.name);
                    println!("  能力: {:?}", plugin.capabilities);
                    println!();
                }
                Ok(None) => println!("\nℹ️  插件 {} 不存在\n", args[2]),
                Err(e) => eprintln!("⚠️  获取插件信息失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_security_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /security config|check|add");
        return;
    }

    match args[1] {
        "config" => match api.get_security_config().await {
            Ok(config) => {
                println!("\n🔒 安全配置:");
                println!("  路径允许: {:?}", config.config.path.include);
                println!("  路径排除: {:?}", config.config.path.exclude);
                println!("  URL允许: {:?}", config.config.url.include);
                println!("  URL排除: {:?}", config.config.url.exclude);
                println!("  命令允许: {:?}", config.config.command.include);
                println!("  命令排除: {:?}", config.config.command.exclude);
                println!();
            }
            Err(e) => eprintln!("⚠️  获取安全配置失败: {:?}", e),
        },
        "check" => {
            if args.len() < 4 {
                eprintln!("⚠️  参数不足，使用 /security check path|url|command <target>");
                return;
            }
            let check_type = args[2];
            let target = args[3..].join(" ");
            match check_type {
                "path" => match api.check_path(&target).await {
                    Ok(safe) => println!("\n🔒 路径 {} 安全: {}\n", target, safe),
                    Err(e) => eprintln!("⚠️  检查路径失败: {:?}", e),
                },
                "url" => match api.check_url(&target).await {
                    Ok(safe) => println!("\n🔒 URL {} 安全: {}\n", target, safe),
                    Err(e) => eprintln!("⚠️  检查URL失败: {:?}", e),
                },
                "command" => match api.check_command(&target).await {
                    Ok(safe) => println!("\n🔒 命令 {} 安全: {}\n", target, safe),
                    Err(e) => eprintln!("⚠️  检查命令失败: {:?}", e),
                },
                _ => eprintln!("⚠️  未知检查类型: {}", check_type),
            }
        }
        "add" => {
            if args.len() < 5 {
                eprintln!("⚠️  参数不足，使用 /security add path|url|command include|exclude <value>");
                return;
            }
            let add_type = args[2];
            let inc_exc = args[3];
            let value = args[4..].join(" ");
            let result = match (add_type, inc_exc) {
                ("path", "include") => api.add_path_include(&value).await,
                ("path", "exclude") => api.add_path_exclude(&value).await,
                ("url", "include") => api.add_url_include(&value).await,
                ("url", "exclude") => api.add_url_exclude(&value).await,
                ("command", "include") => api.add_command_include(&value).await,
                ("command", "exclude") => api.add_command_exclude(&value).await,
                _ => {
                    eprintln!("⚠️  未知参数组合: {} {}", add_type, inc_exc);
                    return;
                }
            };
            match result {
                Ok(_) => println!("\n✅ 安全规则添加成功\n"),
                Err(e) => eprintln!("⚠️  添加安全规则失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}

async fn handle_provider_command(input: &str, api: &Arc<CaelixApiImpl>) {
    let args: Vec<&str> = input.trim().split_whitespace().collect();
    if args.len() < 2 {
        eprintln!("⚠️  缺少子命令，使用 /provider list|models");
        return;
    }

    match args[1] {
        "list" => match api.get_providers().await {
            Ok(providers) => {
                println!("\n🌐 提供者列表:");
                for p in providers {
                    println!("  {} ({})", p.name, p.llm_type);
                    if !p.models.is_empty() {
                        println!("    模型: {}", p.models.join(", "));
                    }
                }
                println!();
            }
            Err(e) => eprintln!("⚠️  获取提供者列表失败: {:?}", e),
        },
        "models" => {
            if args.len() < 3 {
                eprintln!("⚠️  缺少提供者名称，使用 /provider models <name>");
                return;
            }
            match api.get_provider_models(args[2]).await {
                Ok(models) => {
                    println!("\n📦 提供者 {} 的模型:", args[2]);
                    for m in models {
                        println!("  {}", m);
                    }
                    println!();
                }
                Err(e) => eprintln!("⚠️  获取模型列表失败: {:?}", e),
            }
        }
        _ => eprintln!("⚠️  未知子命令: {}", args[1]),
    }
}
