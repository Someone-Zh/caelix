use crate::config::CommandSecurityConfig;
use glob::Pattern;
use std::path::Path;

/// 命令安全检测器
pub struct CommandChecker {
    config: CommandSecurityConfig,
}

impl CommandChecker {
    /// 创建新的 CommandChecker 实例
    pub fn new(config: CommandSecurityConfig) -> Self {
        Self { config }
    }

    /// 检查命令是否允许执行。
    ///
    /// 规则:
    /// 1. 如果任一执行段匹配 exclude, 返回 false
    /// 2. 每个执行段都必须匹配 include, 才返回 true
    /// 3. 否则返回 false
    pub fn is_safe(&self, command: &str) -> bool {
        let Some(programs) = extract_programs(command) else {
            return false;
        };

        for program in &programs {
            for excluded in &self.config.exclude {
                if command_matches(program, excluded) {
                    return false;
                }
            }
        }

        programs.iter().all(|program| {
            self.config
                .include
                .iter()
                .any(|included| command_matches(program, included))
        })
    }

    /// 添加允许命令
    pub fn add_include(&mut self, command: String) {
        if !self.config.include.contains(&command) {
            self.config.include.push(command);
        }
    }

    /// 添加排除命令
    pub fn add_exclude(&mut self, command: String) {
        if !self.config.exclude.contains(&command) {
            self.config.exclude.push(command);
        }
    }

    /// 获取当前配置
    pub fn config(&self) -> &CommandSecurityConfig {
        &self.config
    }
}

fn extract_programs(command: &str) -> Option<Vec<String>> {
    let mut programs = Vec::new();
    for segment in split_command_segments(command) {
        let program = extract_program(&segment)?;
        programs.push(program);
    }

    if programs.is_empty() {
        None
    } else {
        Some(programs)
    }
}

fn split_command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escaped = true;
            continue;
        }

        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            ';' | '|' if !in_single && !in_double => {
                push_segment(&mut segments, &mut current);
                if chars.peek() == Some(&ch) {
                    chars.next();
                }
            }
            '&' if !in_single && !in_double && chars.peek() == Some(&'&') => {
                push_segment(&mut segments, &mut current);
                chars.next();
            }
            _ => current.push(ch),
        }
    }

    push_segment(&mut segments, &mut current);
    segments
}

fn push_segment(segments: &mut Vec<String>, current: &mut String) {
    let segment = current.trim();
    if !segment.is_empty() {
        segments.push(segment.to_string());
    }
    current.clear();
}

fn extract_program(segment: &str) -> Option<String> {
    shell_words::split(segment)
        .ok()
        .and_then(|parts| {
            parts
                .into_iter()
                .skip_while(|part| is_assignment(part))
                .find(|part| !part.trim().is_empty())
        })
        .filter(|program| !program.trim().is_empty())
}

fn is_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| !name.is_empty() && name.chars().all(is_assignment_name_char))
}

fn is_assignment_name_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn command_matches(program: &str, pattern: &str) -> bool {
    if pattern == program {
        return true;
    }

    if let Some(file_name) = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        && pattern == file_name
    {
        return true;
    }

    Pattern::new(pattern)
        .map(|pat| pat.matches(program))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_command_name() {
        let checker = CommandChecker::new(CommandSecurityConfig {
            include: vec!["git".to_string()],
            exclude: vec![],
        });

        assert!(checker.is_safe("git status"));
        assert!(checker.is_safe("/usr/bin/git status"));
        assert!(!checker.is_safe("cargo test"));
    }

    #[test]
    fn test_all_segments_must_be_allowed() {
        let checker = CommandChecker::new(CommandSecurityConfig {
            include: vec!["git".to_string()],
            exclude: vec![],
        });

        assert!(checker.is_safe("git status && git diff"));
        assert!(!checker.is_safe("git status && rm -rf target"));
        assert!(!checker.is_safe("git status; rm -rf target"));
    }

    #[test]
    fn test_excluded_command_wins() {
        let checker = CommandChecker::new(CommandSecurityConfig {
            include: vec!["*".to_string()],
            exclude: vec!["rm".to_string()],
        });

        assert!(!checker.is_safe("rm -rf target"));
        assert!(checker.is_safe("ls -la"));
    }

    #[test]
    fn test_invalid_command() {
        let checker = CommandChecker::new(CommandSecurityConfig {
            include: vec!["git".to_string()],
            exclude: vec![],
        });

        assert!(!checker.is_safe(""));
        assert!(!checker.is_safe("\"unterminated"));
    }

    #[test]
    fn test_env_assignment_before_program() {
        let checker = CommandChecker::new(CommandSecurityConfig {
            include: vec!["cargo".to_string()],
            exclude: vec![],
        });

        assert!(checker.is_safe("RUST_BACKTRACE=1 cargo test"));
    }
}
