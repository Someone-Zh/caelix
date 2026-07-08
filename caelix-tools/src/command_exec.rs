use crate::security::{require_command_allowed, require_path_allowed};
use async_trait::async_trait;
use caelix_api::tool::{Tool, ToolPreCheckResult, ToolResult};
use serde_json::{Value as JsonValue, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Default)]
pub struct CommandExecTool;

#[async_trait]
impl Tool for CommandExecTool {
    fn name(&self) -> &str {
        "exec_command"
    }

    fn description(&self) -> &str {
        "执行命令行命令，返回退出码、stdout 和 stderr"
    }

    fn pre_checks(&self, input: &JsonValue) -> Vec<ToolPreCheckResult> {
        let mut checks = Vec::new();

        if let Some(command) = input["command"].as_str() {
            checks.push(ToolPreCheckResult::command(command));
        }

        if let Some(cwd) = input["cwd"].as_str().filter(|cwd| !cwd.trim().is_empty()) {
            checks.push(ToolPreCheckResult::path(cwd));
        }

        checks
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的完整命令行"
                },
                "cwd": {
                    "type": "string",
                    "description": "命令执行目录，默认继承当前进程工作目录"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "超时时间，默认30秒，最大300秒",
                    "default": DEFAULT_TIMEOUT_SECS,
                    "minimum": 1,
                    "maximum": MAX_TIMEOUT_SECS
                }
            }
        })
    }

    async fn execute(&self, input: JsonValue) -> ToolResult {
        let command = match input["command"].as_str() {
            Some(command) if !command.trim().is_empty() => command,
            _ => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: command".to_string()),
                };
            }
        };

        let timeout_secs = input["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS);

        if let Err(e) = require_command_allowed(command).await {
            return ToolResult {
                output: String::new(),
                error: Some(format!("Command rejected by security policy: {}", e)),
            };
        }

        let parts = match shell_words::split(command) {
            Ok(parts) if !parts.is_empty() => parts,
            Ok(_) => {
                return ToolResult {
                    output: String::new(),
                    error: Some("Missing required parameter: command".to_string()),
                };
            }
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Invalid command syntax: {}", e)),
                };
            }
        };

        let program = &parts[0];
        let args = &parts[1..];
        let mut cmd = Command::new(program);
        cmd.args(args);

        if let Some(cwd) = input["cwd"].as_str().filter(|cwd| !cwd.trim().is_empty()) {
            if let Err(e) = require_path_allowed(cwd).await {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("cwd rejected by security policy: {}", e)),
                };
            }
            cmd.current_dir(cwd);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to execute command: {}", e)),
                };
            }
        };

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut stdout) = stdout {
                stdout.read_to_end(&mut buf).await?;
            }
            Ok::<_, std::io::Error>(buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut stderr) = stderr {
                stderr.read_to_end(&mut buf).await?;
            }
            Ok::<_, std::io::Error>(buf)
        });

        let status =
            match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), child.wait())
                .await
            {
                Ok(Ok(status)) => status,
                Ok(Err(e)) => {
                    return ToolResult {
                        output: String::new(),
                        error: Some(format!("Failed to execute command: {}", e)),
                    };
                }
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return ToolResult {
                        output: String::new(),
                        error: Some(format!("Command timed out after {} seconds", timeout_secs)),
                    };
                }
            };

        let stdout = match stdout_task.await {
            Ok(Ok(buf)) => buf,
            Ok(Err(e)) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read stdout: {}", e)),
                };
            }
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("stdout reader failed: {}", e)),
                };
            }
        };

        let stderr = match stderr_task.await {
            Ok(Ok(buf)) => buf,
            Ok(Err(e)) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("Failed to read stderr: {}", e)),
                };
            }
            Err(e) => {
                return ToolResult {
                    output: String::new(),
                    error: Some(format!("stderr reader failed: {}", e)),
                };
            }
        };

        let stdout = String::from_utf8_lossy(&stdout);
        let stderr = String::from_utf8_lossy(&stderr);
        let exit_code = status.code().unwrap_or(-1);
        let formatted = format!(
            "Exit code: {}\n\nstdout:\n{}\n\nstderr:\n{}",
            exit_code, stdout, stderr
        );

        ToolResult {
            output: formatted,
            error: if status.success() {
                None
            } else {
                Some(format!("Command exited with status {}", exit_code))
            },
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
