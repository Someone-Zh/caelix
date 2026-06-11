/// CLI命令处理模块
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
