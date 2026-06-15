use caelix_runtime::context::CaelixContext;
use caelix_service::CaelixApiImpl;
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化配置 & 日志
    let env_config = caelix_config::EnvConfig::new();

    // 先以日志初始化（必须在任何 tracing 事件之前调用）
    if let Err(e) = caelix_api::logging::init_logging(&env_config.log) {
        eprintln!("[main] init logging failed: {}", e);
    }

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();

    // 检查是否请求帮助
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return Ok(());
    }

    // ---------- logs 子命令：查看日志 ----------
    if args.len() > 1 && args[1] == "logs" {
        run_logs_command(&env_config.log, &args[2..]).await;
        return Ok(());
    }

    // 使用 CaelixContext 初始化
    println!("🔧 初始化 Caelix 上下文...");
    let mut context = CaelixContext::new();
    let plugins = caelix_api::plugins::inventory_plugins(Arc::new(context.clone()));
    context.register_plugins(plugins).await;
    context.init().await.expect("Failed to initialize context");
    let caelix_ctx = Arc::new(context);

    // 创建 API 实现
    let api = Arc::new(CaelixApiImpl::new(caelix_ctx.clone()));

    // 启动信号监听任务
    let session_manager_clone = caelix_ctx.session_manager.clone();
    tokio::spawn(async move {
        signal_ctrl_c(session_manager_clone).await;
    });

    // 根据 features 和参数启动相应的后端
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
            // 如果第一个参数是选项（以-开头），则默认启动CLI后端并传递所有参数
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
        // 没有指定参数，默认启动 CLI 后端
        println!("💻 启动 CLI 后端...");
        caelix_cli::run_cli(api).await?;
    }

    Ok(())
}

/// 打印使用说明
fn print_usage() {
    println!("\n用法:");
    println!("  caelix [options]       - 启动 CLI 界面 (默认)");
    println!("  caelix cli [options]   - 启动 CLI 界面");
    #[cfg(feature = "http-server")]
    println!("  caelix http [port]     - 启动 HTTP 服务器 (默认端口 3000)");
    #[cfg(feature = "tui")]
    println!("  caelix tui             - 启动 TUI 界面");
    println!("  caelix logs [sub]      - 日志管理");
    println!("    logs ls               列出所有日志文件");
    println!("    logs show [tail N]    显示当前日志 (默认 50 行，-n <数字> 指定行数)");
    println!("    logs follow           实时跟随当前日志 (tail -f)");
    println!("    logs clean            删除所有日志文件");
    println!("    logs dir              显示日志目录");
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

/// Ctrl+C 信号处理器
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

/// Flush 待持久化的消息
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

// ---------- 日志管理命令 ----------

/// 主入口：logs 子命令
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
            // 解析 -n 参数
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
    // 先跳到文件末尾
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
                    // 同样尝试美化 JSON 输出
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
