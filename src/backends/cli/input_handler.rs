/// 输入处理和多行输入支持模块

use std::io::{self, BufRead, Write};

/// 读取用户输入,支持多行输入
/// 用户输入直到按下 Ctrl+D (EOF) 才算结束
/// 返回完整的输入内容
pub fn read_multiline_input() -> io::Result<Option<String>> {
    let stdin = io::stdin();
    let mut lines = Vec::new();
    
    print!("> ");
    io::stdout().flush()?;
    
    for line in stdin.lock().lines() {
        match line {
            Ok(l) => {
                // 收集所有行,直到遇到 EOF (Ctrl+D)
                lines.push(l);
            }
            Err(_e) => {
                // Ctrl+D 导致 EOF
                if lines.is_empty() {
                    return Ok(None);
                }
                break;
            }
        }
    }
    
    if lines.is_empty() {
        Ok(None)
    } else {
        // 用换行符连接所有行
        let input = lines.join("\n");
        Ok(Some(input))
    }
}

/// 简单的单行输入读取（用于快速测试）
#[allow(dead_code)]
pub fn read_single_line() -> io::Result<Option<String>> {
    let mut input = String::new();
    print!("> ");
    io::stdout().flush()?;
    
    let bytes_read = io::stdin().read_line(&mut input)?;
    
    if bytes_read == 0 {
        Ok(None)
    } else {
        // 移除末尾的换行符
        let trimmed = input.trim_end_matches(|c| c == '\n' || c == '\r').to_string();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed))
        }
    }
}
