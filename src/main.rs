mod base;
mod manager;
mod runtime;
mod config;
mod enhancement;
mod api;
mod backends;
mod utils;

use std::sync::Arc;
use crate::config::CaelixContext;
use crate::api::{CaelixApi, CaelixApiImpl};
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 tracing（控制台只显示 INFO 以上级别）
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    
    // 检查是否请求帮助
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return Ok(());
    }
    
    // 使用 CaelixContext 初始化
    println!("🔧 初始化 Caelix 上下文...");
    let context = CaelixContext::new();
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
        // 用户显式指定了后端或选项
        match args[1].as_str() {
            "cli" => {
                println!("💻 启动 CLI 后端...");
                backends::cli::run_cli(api).await?;
            }
            #[cfg(feature = "http-server")]
            "http" => {
                println!("🌐 启动 HTTP Server 后端...");
                let port = if args.len() > 2 {
                    args[2].parse::<u16>().unwrap_or(3000)
                } else {
                    3000
                };
                backends::http::start_http_server(api, port).await?;
            }
            #[cfg(feature = "tui")]
            "tui" => {
                println!("🖥️  启动 TUI 后端...");
                backends::tui::run_tui(api).await?;
            }
            // 如果第一个参数是选项（以-开头），则默认启动CLI后端并传递所有参数
            arg if arg.starts_with('-') => {
                println!("💻 启动 CLI 后端...");
                backends::cli::run_cli(api).await?;
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
        backends::cli::run_cli(api).await?;
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
async fn signal_ctrl_c(session_manager: Arc<crate::runtime::message::manager::SessionManager>) {
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n⚠️  收到退出信号，正在保存未持久化的消息...");
            
            // Flush 所有 agent_buffers 中的 Chunk 消息
            flush_pending_messages(session_manager).await;
            
            println!("✅ 消息已保存，安全退出");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("❌ 无法监听 Ctrl+C 信号: {}", err);
        }
    }
}

/// Flush 待持久化的消息
async fn flush_pending_messages(session_manager: Arc<crate::runtime::message::manager::SessionManager>) {
    use crate::runtime::message::agent_message::AgentMessageType;
    
    // 访问 agent_buffers 并持久化所有待处理的 Chunk 消息
    let buffers = session_manager.get_agent_buffers().read().await;
    for ((_session_id, _request_id, _span_id), messages) in buffers.iter() {
        for msg in messages {
            // 只持久化 Msg 类型的消息（Chunk 不需要持久化）
            if msg.r#type == AgentMessageType::Msg {
                if let Err(e) = session_manager.get_storage().append_agent_message(msg).await {
                    eprintln!("⚠️  保存消息失败: {:?}", e);
                }
            }
        }
    }
}

/// 根据启用的 features 自动启动默认后端（已弃用，CLI 为默认）
#[allow(dead_code)]
async fn start_default_backend(_api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    // CLI 现在是默认后端，此函数保留用于兼容性
    backends::cli::run_cli(_api).await
}

#[allow(dead_code)] // 演示模式使用
async fn run_demo(api: Arc<CaelixApiImpl>) {
    println!("\n🚀 Caelix 演示模式");
    println!("==================");
    
    // 获取默认配置
    let default_provider = api.get_default_provider();
    let default_model = api.get_default_model();
    println!("默认提供者: {}", default_provider);
    println!("默认模型: {}", default_model);
    
    // 创建会话
    let session_id = api.create_session().await;
    println!("\n✅ 创建会话: {}", session_id);
    
    // 获取 agent 列表
    let agents = api.list_agents().await;
    println!("\n📋 可用的 Agents:");
    for agent in agents {
        println!("  - {}", agent);
    }
    
    println!("\n💡 提示: 使用 'caelix http' 或 'caelix tui' 启动相应的后端");
}