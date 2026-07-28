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

    let has_tui = args.iter().any(|a| a == "--tui");
    let has_http = args.iter().any(|a| a == "--http");

    if has_tui && has_http {
        eprintln!("❌ 不能同时指定 --tui 和 --http");
        std::process::exit(1);
    }

    if has_http {
        #[cfg(feature = "http-server")]
        {
            let port = args
                .iter()
                .position(|a| a == "--http")
                .and_then(|i| args.get(i + 1))
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(3000);

            println!("🌐 启动 HTTP Server 后端...");
            caelix_http::start_http_server(api, port).await?;
        }
        #[cfg(not(feature = "http-server"))]
        {
            eprintln!("❌ http-server feature 未启用，请使用 --features http-server 编译");
            std::process::exit(1);
        }
    } else if has_tui {
        #[cfg(feature = "tui")]
        {
            println!("🖥️  启动 TUI 后端...");
            caelix_tui::run_tui(api).await?;
        }
        #[cfg(not(feature = "tui"))]
        {
            eprintln!("❌ tui feature 未启用，请使用 --features tui 编译");
            std::process::exit(1);
        }
    } else {
        println!("💻 启动 CLI 后端...");
        caelix_cli::run_cli(api).await?;
    }

    Ok(())
}

fn print_usage() {
    println!("\n用法:");
    println!("  caelix [子命令] [选项]    启动 CLI 界面 (默认)");
    println!("  caelix --tui              启动 TUI 界面");
    println!("  caelix --http [port]      启动 HTTP 服务器 (默认端口 3000)");
    println!("\nCLI 子命令:");
    println!("  chat    对话聊天");
    println!("  tool    工具执行");
    println!("  list    列表查询 (sessions, agents, tools, skills, commands, hooks, plugins, providers)");
    println!("  session 会话管理");
    println!("  variable 变量管理");
    println!("  agent   智能体管理");
    println!("  skill   技能管理");
    println!("  command 命令管理");
    println!("  hook    Hook 管理");
    println!("  plugin  插件管理");
    println!("  security 安全管理");
    println!("  provider 提供商管理");
    println!("  usage   Token 用量");
    println!("  task    任务管理");
    println!("  memory  记忆管理");
    println!("  logs    日志管理");
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
