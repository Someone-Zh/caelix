use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// TUI 事件
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// 用户输入文本
    Input(String),
    /// 按键事件
    Key(KeyEvent),
    /// 退出应用
    Quit,
    /// 发送消息（Command+Enter）
    Send,
    /// 换行（普通Enter）
    NewLine,
    /// 调整大小
    Resize(u16, u16),
}

/// 事件处理器
pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate),
        }
    }

    /// 等待并返回下一个事件
    pub fn next(&self) -> Result<TuiEvent, std::io::Error> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    // 检查 Command/Ctrl 修饰键
                    let is_command = key.modifiers.contains(KeyModifiers::CONTROL) 
                        || key.modifiers.contains(KeyModifiers::SUPER);
                    
                    if is_command {
                        match key.code {
                            KeyCode::Char('c') | KeyCode::Char('q') => Ok(TuiEvent::Quit),
                            KeyCode::Enter => Ok(TuiEvent::Send),  // Command+Enter 发送
                            _ => Ok(TuiEvent::Key(key)),
                        }
                    } else {
                        match key.code {
                            KeyCode::Enter => Ok(TuiEvent::NewLine),  // 普通Enter换行
                            KeyCode::Esc => Ok(TuiEvent::Key(key)),  // Esc作为普通按键处理
                            _ => Ok(TuiEvent::Key(key)),
                        }
                    }
                }
                Event::Resize(width, height) => Ok(TuiEvent::Resize(width, height)),
                _ => Ok(TuiEvent::Key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE))),
            }
        } else {
            Ok(TuiEvent::Key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE)))
        }
    }
}
