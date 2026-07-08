//! CLI命令处理模块
use std::sync::Arc;

use caelix_service::{CaelixApi, CaelixApiImpl};

/// 检查是否是退出命令
pub fn is_quit_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/quit" || trimmed == "/exit" || trimmed == "/q"
}

/// 处理CLI命令，返回是否应该退出
pub fn handle_command(input: &str) -> bool {
    if is_quit_command(input) {
        println!("\n👋 再见！");
        return true;
    }

    // 未来可以在这里添加更多命令
    // 例如: /help, /clear, /session 等

    false
}

/// 检查是否是 usage 相关命令
pub fn is_usage_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/usage" || trimmed.starts_with("/usage ")
}

/// 处理 usage 命令
///
/// 支持的格式：
/// - `/usage`：显示当前 session 的累计用量
/// - `/usage --session <id>`：显示指定 session 的累计用量
/// - `/usage --global`：显示全局用量（按 provider/model 汇总）
pub async fn handle_usage_command(input: &str, session_id: &str, api: &Arc<CaelixApiImpl>) {
    let trimmed = input.trim();
    let args: Vec<&str> = trimmed.split_whitespace().collect();

    // 解析参数
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

    // 显示 session 维度
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
