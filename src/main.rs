mod base;
mod manager;
mod runtime;
mod config;
mod enhancement;
mod api;
mod backends;

use std::sync::Arc;
use crate::config::CaelixContext;
use crate::api::{CaelixApi, CaelixApiImpl};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    
    // 使用 CaelixContext 初始化
    println!("🔧 初始化 Caelix 上下文...");
    let context = CaelixContext::new();
    context.init().await.expect("Failed to initialize context");
    let caelix_ctx = Arc::new(context);
    
    // 创建 API 实现
    let api = Arc::new(CaelixApiImpl::new(caelix_ctx.clone()));
    
    // 根据 features 和参数启动相应的后端
    if args.len() > 1 {
        // 用户显式指定了后端
        match args[1].as_str() {
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
            _ => {
                eprintln!("❌ 未知的后端: {}", args[1]);
                print_usage();
                std::process::exit(1);
            }
        }
    } else {
        // 没有指定参数，根据启用的 features 自动选择
        start_default_backend(api).await?;
    }
    
    Ok(())
}

/// 打印使用说明
fn print_usage() {
    println!("\n用法:");
    #[cfg(feature = "http-server")]
    println!("  caelix http [port]  - 启动 HTTP 服务器 (默认端口 3000)");
    #[cfg(feature = "tui")]
    println!("  caelix tui          - 启动 TUI 界面");
    println!("\n可用的 features:");
    #[cfg(feature = "http-server")]
    println!("  - http-server");
    #[cfg(feature = "tui")]
    println!("  - tui");
}

/// 根据启用的 features 自动启动默认后端
async fn start_default_backend(api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(all(feature = "http-server", feature = "tui"))]
    {
        // 同时启用了多个 features，需要用户指定
        eprintln!("⚠️  检测到多个后端可用，请指定要启动的后端:");
        print_usage();
        std::process::exit(1);
    }
    
    #[cfg(all(feature = "http-server", not(feature = "tui")))]
    {
        println!("🌐 启动 HTTP Server 后端...");
        backends::http::start_http_server(api, 3000).await?;
    }
    
    #[cfg(all(feature = "tui", not(feature = "http-server")))]
    {
        println!("🖥️  启动 TUI 后端...");
        backends::tui::run_tui(api).await?;
    }
    
    #[cfg(not(any(feature = "http-server", feature = "tui")))]
    {
        // 没有启用任何 features，运行演示模式
        println!("⚠️  未启用任何后端，运行演示模式...");
        run_demo(api).await;
    }
    
    Ok(())
}
async fn run_demo(api: Arc<CaelixApiImpl>) {
    println!("\n🚀 Caelix 演示模式");
    println!("==================");
    
    // 获取默认配置
    let default_provider = api.get_default_provider();
    let default_model = api.get_default_model();
    println!("默认提供者: {}", default_provider);
    println!("默认模型: {}", default_model);
    
    // 创建会话
    let session_id = api.create_session();
    println!("\n✅ 创建会话: {}", session_id);
    
    // 获取 agent 列表
    let agents = api.list_agents().await;
    println!("\n📋 可用的 Agents:");
    for agent in agents {
        println!("  - {}", agent);
    }
    
    println!("\n💡 提示: 使用 'caelix http' 或 'caelix tui' 启动相应的后端");
}