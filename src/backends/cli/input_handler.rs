/// 输入处理和多行输入支持模块
use std::io::{self, BufRead, Write};

/// 读取用户输入,支持多行输入
/// 用户使用回车换行继续输入，输入空行则提交结束
/// 返回完整的输入内容
pub fn read_multiline_input() -> io::Result<Option<String>> {
    let stdin = io::stdin();
    let mut lines = Vec::new();
    
    loop {
        print!("> ");
        io::stdout().flush()?;
        
        let mut line = String::new();
        let bytes_read = stdin.lock().read_line(&mut line)?;
        
        if bytes_read == 0 {
            // EOF (Ctrl+D)
            if lines.is_empty() {
                return Ok(None);
            }
            break;
        }
        
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        
        if trimmed.is_empty() && !lines.is_empty() {
            // 空行且已有内容，提交
            break;
        }
        
        lines.push(trimmed);
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
        let trimmed = input.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed))
        }
    }
}
