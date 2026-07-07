use caelix_runtime::context::CaelixContext;
use caelix_service::CaelixApiImpl;
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_config = caelix_config::EnvConfig::new();

    if let Err(e) = caelix_api::logging::init_logging(&env_config.log) {
        eprintln!("[main] init logging failed: {}", e);
    }

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return Ok(());
    }

    if args.len() > 1 && args[1] == "logs" {
        run_logs_command(&env_config.log, &args[2..]).await;
        return Ok(());
    }

    if args.len() > 1 && args[1] == "memory" {
        let context = CaelixContext::new();
        let caelix_ctx = Arc::new(context);
        run_memory_command(caelix_ctx, &args[2..]).await;
        return Ok(());
    }

    println!("🔧 初始化 Caelix 上下文...");
    let mut context = CaelixContext::new();
    let plugins = caelix_api::plugins::inventory_plugins(Arc::new(context.clone()));
    context.register_plugins(plugins).await;
    context.init().await.expect("Failed to initialize context");
    let caelix_ctx = Arc::new(context);

    let api = Arc::new(CaelixApiImpl::new(caelix_ctx.clone()));

    let session_manager_clone = caelix_ctx.session_manager.clone();
    tokio::spawn(async move {
        signal_ctrl_c(session_manager_clone).await;
    });

    if args.len() > 1 {
        match args[1].as_str() {
            "cli" => {
                println!("💻 启动 CLI 后端...");
                caelix_cli::run_cli(api).await?;
            }
            #[cfg(feature = "http-server")]
            "http" => {
                println!("🌐 启动 HTTP Server 后端...");
                let port = if args.len() > 2 {
                    args[2].parse::<u16>().unwrap_or(3000)
                } else {
                    3000
                };
                caelix_http::start_http_server(api, port).await?;
            }
            #[cfg(feature = "tui")]
            "tui" => {
                println!("🖥️  启动 TUI 后端...");
                caelix_tui::run_tui(api).await?;
            }
            arg if arg.starts_with('-') => {
                println!("💻 启动 CLI 后端...");
                caelix_cli::run_cli(api).await?;
            }
            _ => {
                eprintln!("❌ 未知的后端: {}", args[1]);
                print_usage();
                std::process::exit(1);
            }
        }
    } else {
        println!("💻 启动 CLI 后端...");
        caelix_cli::run_cli(api).await?;
    }

    Ok(())
}

fn print_usage() {
    println!("\n用法:");
    println!("  caelix [options]       - 启动 CLI 界面 (默认)");
    println!("  caelix cli [options]   - 启动 CLI 界面");
    #[cfg(feature = "http-server")]
    println!("  caelix http [port]     - 启动 HTTP 服务器 (默认端口 3000)");
    #[cfg(feature = "tui")]
    println!("  caelix tui             - 启动 TUI 界面");
    println!("  caelix logs [sub]      - 日志管理");
    println!("  caelix memory [sub]    - 记忆系统");
    println!("\nCLI 选项:");
    println!("  -s, --session <ID>     - 指定会话 ID");
    println!("  -a, --agent <NAME>     - 指定使用的 Agent");
    println!("  -p, --provider <NAME>  - 指定提供商");
    println!("  -m, --model <NAME>     - 指定模型");
    println!("\n可用的 features:");
    #[cfg(feature = "http-server")]
    println!("  - http-server");
    #[cfg(feature = "tui")]
    println!("  - tui");
}

async fn signal_ctrl_c(session_manager: Arc<caelix_message::SessionManager>) {
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n⚠️  收到退出信号，正在保存未持久化的消息...");
            flush_pending_messages(session_manager).await;
            println!("✅ 消息已保存，安全退出");
            std::process::exit(0);
        }
        Err(err) => {
            tracing::error!(error = %err, "无法监听 Ctrl+C 信号");
        }
    }
}

async fn flush_pending_messages(session_manager: Arc<caelix_message::SessionManager>) {
    use caelix_api::message::AgentMessageType;

    let buffers = session_manager.get_agent_buffers().read().await;
    for ((_session_id, _request_id, _span_id), messages) in buffers.iter() {
        for msg in messages {
            if msg.r#type == AgentMessageType::Msg
                && let Err(e) = session_manager
                    .get_storage()
                    .append_agent_message(msg)
                    .await
            {
                tracing::warn!(error = %e, "保存消息失败");
            }
        }
    }
}

async fn run_memory_command(_caelix_ctx: Arc<CaelixContext>, args: &[String]) {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");

    let config = caelix_memory::schema::MemoryVaultConfig::default();
    let vault = caelix_memory::MemoryVault::new(config);
    if let Err(e) = vault.init().await {
        eprintln!("❌ 初始化 MemoryVault 失败: {}", e);
        return;
    }

    match sub {
        "recall" => {
            if args.len() < 2 {
                eprintln!("❌ 用法: caelix memory recall <query> [--top-k N]");
                return;
            }
            let query = args[1].as_str();
            let mut top_k = 5;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--top-k" | "-k" => {
                        if i + 1 < args.len() {
                            top_k = args[i + 1].parse().unwrap_or(5);
                        }
                        i += 2;
                    }
                    _ => i += 1,
                }
            }

            match vault.recall(query, top_k).await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("(未找到相关记忆)");
                        return;
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
                        let conf = result.confidence.map(|c| format!(" ({:.0}%)", c * 100.0)).unwrap_or_default();
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
        "write" => {
            if args.len() < 2 {
                eprintln!("❌ 用法: caelix memory write <content> [--source chat|meeting|tweet|paper|note] [--tag TAG...]");
                return;
            }
            let content = args[1].as_str();
            
            let mut source_str = "chat";
            let mut tags = Vec::new();
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--source" | "-s" => {
                        if i + 1 < args.len() {
                            source_str = args[i + 1].as_str();
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--tag" | "-t" => {
                        if i + 1 < args.len() {
                            tags.push(args[i + 1].clone());
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => i += 1,
                }
            }

            let source = match source_str {
                "meeting" => caelix_memory::schema::RawSource::Meeting,
                "tweet" => caelix_memory::schema::RawSource::Tweet,
                "paper" => caelix_memory::schema::RawSource::Paper,
                "note" => caelix_memory::schema::RawSource::Note,
                _ => caelix_memory::schema::RawSource::Chat,
            };

            let today = chrono::Utc::now().date_naive();
            let heading = chrono::Utc::now().format("%H:%M").to_string();

            match vault.write_raw(today, source, tags, &heading, content).await {
                Ok(_) => println!("✅ 已写入 Raw 层"),
                Err(e) => eprintln!("❌ 写入失败: {}", e),
            }
        }
        "promote" => {
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--raw" => {
                        if i + 1 < args.len() {
                            let file = args[i + 1].as_str();
                            println!("🔄 手动触发 Raw→Wiki 晋升: {}", file);
                            println!("(P2 阶段实现，当前仅记录)");
                        }
                        i += 2;
                    }
                    "--wiki" => {
                        if i + 1 < args.len() {
                            let entity = args[i + 1].as_str();
                            println!("🔄 手动触发 Wiki→Axiom 晋升: {}", entity);
                            println!("(P2 阶段实现，当前仅记录)");
                        }
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
        }
        "flags" => {
            let all = args.contains(&"--all".to_string());

            let conflicts = match vault.list_conflicts(all).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("❌ 获取冲突列表失败: {}", e);
                    return;
                }
            };

            let candidates = match vault.list_candidates(all).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("❌ 获取候选列表失败: {}", e);
                    return;
                }
            };

            println!("==================================");
            println!("  ⚠️  冲突与候选列表");
            println!("==================================");

            if !conflicts.is_empty() {
                println!("\n  🚫 冲突 ({})", conflicts.len());
                println!("  ------------------------------");
                for conflict in &conflicts {
                    let status_icon = if conflict.status == "Pending" { "⏳" } else { "✅" };
                    println!("  {} {} [{}] {} - {}", status_icon, conflict.id, conflict.r#type, conflict.entity, conflict.field.as_deref().unwrap_or(""));
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
                    println!("  {} {} (confidence: {:.0}%)", status_icon, candidate.id, candidate.confidence * 100.0);
                    println!("       {}", candidate.preview);
                }
            }

            if conflicts.is_empty() && candidates.is_empty() {
                println!("(暂无冲突或候选)");
            }
            println!("\n==================================");
        }
        "rebuild-index" => {
            println!("🔄 正在重建反向索引...");
            match vault.rebuild_index().await {
                Ok(_) => println!("✅ 索引重建完成"),
                Err(e) => eprintln!("❌ 索引重建失败: {}", e),
            }
        }
        "stats" => {
            match vault.stats().await {
                Ok(stats) => {
                    println!("==================================");
                    println!("  📊 Memory Vault 统计");
                    println!("==================================");
                    println!("  Raw 文件数        : {}", stats.raw_files);
                    println!("  Wiki 实体数       : {}", stats.wiki_entities);
                    println!("  Wiki 事件数       : {}", stats.wiki_events);
                    println!("  Axiom 总数        : {} (活跃: {})", stats.axioms, stats.axioms_active);
                    println!("  待处理冲突        : {}", stats.pending_conflicts);
                    println!("  Axiom 候选        : {}", stats.pending_candidates);
                    println!("  待创建链接        : {}", stats.pending_links);
                    println!("  LLM 预算          : {}/{}", stats.llm_budget_used, stats.llm_budget_total);
                    println!("==================================");
                }
                Err(e) => eprintln!("❌ 获取统计失败: {}", e),
            }
        }
        "axioms" => {
            let include_deprecated = args.contains(&"--include-deprecated".to_string());
            
            match vault.list_axioms(include_deprecated).await {
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
                        println!("     创建于: {}", axiom.created_at.format("%Y-%m-%d %H:%M"));
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
        "budget" => {
            let info = vault.get_budget_info().await;
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
        "--help" | "-h" | "" => {
            print_memory_help();
        }
        other => {
            eprintln!("❌ 未知的 memory 子命令: {}", other);
            print_memory_help();
        }
    }
}

fn print_memory_help() {
    println!("\n记忆系统子命令用法:");
    println!("  caelix memory recall <query> [--top-k N]    检索记忆（默认返回 5 条）");
    println!("  caelix memory write <content> [选项]          写入 Raw 层");
    println!("    --source chat|meeting|tweet|paper|note    来源类型（默认 chat）");
    println!("    --tag TAG                                 添加标签");
    println!("  caelix memory promote --raw <file>          手动触发 Raw→Wiki 晋升");
    println!("  caelix memory promote --wiki <entity>       手动触发 Wiki→Axiom 晋升");
    println!("  caelix memory flags [--all]                 列出冲突和 Axiom 候选");
    println!("  caelix memory rebuild-index                 重建反向索引");
    println!("  caelix memory stats                         显示统计信息");
    println!("  caelix memory axioms [--include-deprecated] 查看 Axiom 列表");
    println!("  caelix memory budget                        查看 LLM 预算使用情况");
}

async fn run_logs_command(config: &caelix_api::logging::LogConfig, sub_args: &[String]) {
    let sub = sub_args.first().map(|s| s.as_str()).unwrap_or("ls");

    match sub {
        "dir" => {
            println!("日志目录: {}", config.dir.display());
        }
        "ls" | "list" => {
            list_logs(&config.dir);
        }
        "show" => {
            let mut n: usize = 50;
            let mut i = 1;
            while i < sub_args.len() {
                if sub_args[i] == "-n" && i + 1 < sub_args.len() {
                    if let Ok(v) = sub_args[i + 1].parse::<usize>() {
                        n = v;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            show_current_log(&config.dir, n);
        }
        "follow" | "tailf" => {
            follow_log(&config.dir).await;
        }
        "clean" => {
            clean_logs(&config.dir);
        }
        "--help" | "-h" => {
            print_logs_help();
        }
        other => {
            eprintln!("❌ 未知的 logs 子命令: {}", other);
            print_logs_help();
        }
    }
}

fn print_logs_help() {
    println!("\n日志子命令用法:");
    println!("  caelix logs dir       显示日志目录路径");
    println!("  caelix logs ls        列出所有日志文件");
    println!("  caelix logs show [-n N]  显示当前日志最后 N 行 (默认 50)");
    println!("  caelix logs follow    实时跟随当前日志 (Ctrl+C 退出)");
    println!("  caelix logs clean     删除所有日志文件");
}

fn list_logs(dir: &std::path::Path) {
    use std::fs;

    println!("日志目录: {}", dir.display());
    println!();

    if !dir.exists() {
        println!("(目录不存在)");
        return;
    }

    match fs::read_dir(dir) {
        Ok(entries) => {
            let mut files: Vec<(String, u64, String)> = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Some(name) = path.file_name().and_then(|s| s.to_str())
                    && name.starts_with("caelix.") && name.ends_with(".log")
                {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| format!("{:?}", t))
                        .unwrap_or_else(|| "-".to_string());
                    files.push((name.to_string(), size, modified));
                }
            }
            files.sort_by(|a, b| a.0.cmp(&b.0));
            if files.is_empty() {
                println!("(暂无日志文件)");
                return;
            }
            println!("{:<40} {:>12} 修改时间", "文件名", "大小");
            println!("{}", "-".repeat(80));
            for (name, size, modified) in &files {
                println!("{:<40} {:>10}B  {}", name, size, modified);
            }
            let total: u64 = files.iter().map(|(_, s, _)| *s).sum();
            println!();
            println!("共 {} 个文件，合计 {} KB", files.len(), total / 1024);
        }
        Err(e) => {
            eprintln!("❌ 读取日志目录失败: {}", e);
        }
    }
}

fn show_current_log(dir: &std::path::Path, n: usize) {
    use std::fs;
    use std::io::{BufRead, BufReader};

    let current = dir.join("caelix.current.log");
    if !current.exists() {
        println!("(当前日志文件不存在)");
        return;
    }
    println!("文件: {}", current.display());
    println!("{}", "=".repeat(60));

    match fs::File::open(&current) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
            let start = if lines.len() > n { lines.len() - n } else { 0 };
            for line in lines.iter().skip(start) {
                if line.trim_start().starts_with('{')
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                {
                    let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
                    let lvl = v.get("level").and_then(|t| t.as_str()).unwrap_or("");
                    let target = v.get("target").and_then(|t| t.as_str()).unwrap_or("");
                    let msg = v.get("message").and_then(|t| t.as_str()).unwrap_or("");
                    let fields: Option<serde_json::Value> = v.get("fields").cloned();
                    let span: Option<serde_json::Value> = v.get("span").cloned();
                    let spans: Option<serde_json::Value> = v.get("spans").cloned();
                    print!("[{}] {:<5} [{}] {}", ts, lvl, target, msg);
                    if let Some(f) = &fields
                        && !f.is_null() && !f.as_object().map(|o| o.is_empty()).unwrap_or(true)
                    {
                        print!(" | {}", f);
                    }
                    if let Some(s) = &span
                        && !s.is_null()
                    {
                        print!(" | span={}", s);
                    }
                    if let Some(s) = &spans
                        && let Some(arr) = s.as_array()
                        && !arr.is_empty()
                    {
                        print!(" | spans={}", s);
                    }
                    println!();
                    continue;
                }
                println!("{}", line);
            }
        }
        Err(e) => {
            eprintln!("❌ 打开日志文件失败: {}", e);
        }
    }
}

async fn follow_log(dir: &std::path::Path) {
    use std::fs;
    use std::io::{Seek, SeekFrom};

    let current = dir.join("caelix.current.log");
    if !current.exists() {
        println!("(当前日志文件不存在，等待写入...)");
    }
    println!("跟随日志: {} (Ctrl+C 退出)", current.display());
    println!("{}", "=".repeat(60));

    let mut pos: u64 = 0;
    if let Ok(file) = fs::File::open(&current)
        && let Ok(meta) = file.metadata()
    {
        pos = meta.len();
    }

    loop {
        if let Ok(mut file) = fs::File::open(&current)
            && file.seek(SeekFrom::Start(pos)).is_ok()
        {
            use std::io::Read;
            let mut buf = String::new();
            if file.read_to_string(&mut buf).is_ok() && !buf.is_empty() {
                pos += buf.len() as u64;
                for line in buf.lines() {
                    if line.trim_start().starts_with('{')
                        && let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                    {
                        let ts =
                            v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
                        let lvl = v.get("level").and_then(|t| t.as_str()).unwrap_or("");
                        let target =
                            v.get("target").and_then(|t| t.as_str()).unwrap_or("");
                        let msg =
                            v.get("message").and_then(|t| t.as_str()).unwrap_or("");
                        let fields: Option<serde_json::Value> = v.get("fields").cloned();
                        print!("[{}] {:<5} [{}] {}", ts, lvl, target, msg);
                        if let Some(f) = &fields
                            && !f.is_null()
                                && !f.as_object().map(|o| o.is_empty()).unwrap_or(true)
                        {
                            print!(" | {}", f);
                        }
                        println!();
                        continue;
                    }
                    println!("{}", line);
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

fn clean_logs(dir: &std::path::Path) {
    use std::fs;
    use std::io::{self, Write};

    if !dir.exists() {
        println!("(目录不存在)");
        return;
    }

    print!("确认要删除 {} 下的所有日志文件吗？(y/N): ", dir.display());
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let answer = input.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        println!("已取消");
        return;
    }

    let mut count: usize = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|s| s.to_str())
                && name.starts_with("caelix.") && name.ends_with(".log")
            {
                if fs::remove_file(&path).is_ok() {
                    count += 1;
                    println!("已删除: {}", name);
                } else {
                    eprintln!("删除失败: {}", name);
                }
            }
        }
    }
    println!("共删除 {} 个文件", count);
}