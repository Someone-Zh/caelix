//! CLI 帮助文档模块
//!
//! 此模块包含所有 CLI 子命令的帮助文本，可被其他模块（如 HTTP Server）复用。

pub const CLI_HELP: &str = r#"
Caelix - AI Agent 命令行工具

用法:
  caelix <子命令> [选项]

可用子命令:
  chat        对话聊天
  tool        工具执行
  list        列表查询 (sessions, agents, tools, skills, commands, hooks, plugins, providers)
  session     会话管理
  variable    变量管理
  agent       智能体管理
  skill       技能管理
  command     命令管理
  hook        Hook 管理
  plugin      插件管理
  security    安全管理
  provider    提供商管理
  usage       Token 用量
  task        任务管理
  memory      记忆管理
  logs        日志管理
  help        显示此帮助信息

全局选项:
  --tui       启动 TUI 界面
  --http      启动 HTTP 服务器
  -h, --help  显示帮助信息
"#;

pub const CHAT_HELP: &str = r#"
对话聊天

用法:
  caelix chat [选项]

选项:
  -s, --session <ID>       指定会话 ID（未提供则自动创建）
  -a, --agent <NAME>       指定使用的 Agent
  -p, --provider <NAME>    指定提供商
  -m, --model <NAME>       指定模型
  -c, --content <TEXT>     对话内容（必需）
  -h, --help               显示帮助信息

示例:
  caelix chat -c "你好，请介绍一下 Rust"
  caelix chat -s my-session -a code_executor -c "帮我优化这段代码"
"#;

pub const TOOL_HELP: &str = r#"
工具执行

用法:
  caelix tool <操作> [选项]

操作:
  exec <名称> [参数...]    执行指定工具
  list                     列出所有可用工具
  info <名称>              查看工具详情
  -h, --help               显示帮助信息

工具执行选项:
  --args <JSON>            以 JSON 格式传递工具参数
  -h, --help               显示帮助信息

示例:
  caelix tool list
  caelix tool info file_read
  caelix tool exec file_read --path /path/to/file
  caelix tool exec file_search --args '{"pattern":"test","path":"."}'
"#;

pub const LIST_HELP: &str = r#"
列表查询

用法:
  caelix list <类型>

类型:
  sessions      列出所有会话
  agents        列出所有智能体
  tools         列出所有工具
  skills        列出所有技能
  commands      列出所有命令
  hooks         列出所有 Hook
  plugins       列出所有插件
  providers     列出所有提供商
  -h, --help    显示帮助信息

示例:
  caelix list sessions
  caelix list tools
  caelix list agents
"#;

pub const SESSION_HELP: &str = r#"
会话管理

用法:
  caelix session <操作> [选项]

操作:
  list                    列出所有会话
  info <ID>               查看会话详情（消息历史）
  create [ID]             创建新会话（可选指定 ID）
  delete <ID>             删除会话
  stop <ID>               停止会话中运行的 Agent
  set-provider <ID> <P>   设置会话的提供商
  set-model <ID> <M>      设置会话的模型
  -h, --help              显示帮助信息

示例:
  caelix session list
  caelix session info my-session
  caelix session create my-session
"#;

pub const VARIABLE_HELP: &str = r#"
变量管理

用法:
  caelix variable <操作> [选项]

操作:
  list                        列出所有全局变量
  get <KEY>                   获取变量值
  set <KEY> <VALUE>           设置变量
  delete <KEY>                删除变量
  space <SPACE> list          列出空间变量
  space <SPACE> get <KEY>     获取空间变量值
  space <SPACE> set <K> <V>   设置空间变量
  space <SPACE> delete <KEY>  删除空间变量
  replace <TEXT>              替换文本中的变量
  -h, --help                  显示帮助信息

示例:
  caelix variable list
  caelix variable set api_key abc123
  caelix variable get api_key
"#;

pub const AGENT_HELP: &str = r#"
智能体管理

用法:
  caelix agent <操作> [选项]

操作:
  list          列出所有智能体
  info <NAME>   查看智能体详情
  -h, --help    显示帮助信息

示例:
  caelix agent list
  caelix agent info code_executor
"#;

pub const SKILL_HELP: &str = r#"
技能管理

用法:
  caelix skill <操作> [选项]

操作:
  list          列出所有技能
  info <NAME>   查看技能详情
  -h, --help    显示帮助信息

示例:
  caelix skill list
  caelix skill info my_skill
"#;

pub const COMMAND_HELP: &str = r#"
命令管理

用法:
  caelix command <操作> [选项]

操作:
  list          列出所有命令
  info <NAME>   查看命令详情
  -h, --help    显示帮助信息

示例:
  caelix command list
  caelix command info my_command
"#;

pub const HOOK_HELP: &str = r#"
Hook 管理

用法:
  caelix hook <操作> [选项]

操作:
  list          列出所有 Hook
  info <NAME>   查看 Hook 详情
  -h, --help    显示帮助信息

示例:
  caelix hook list
  caelix hook info my_hook
"#;

pub const PLUGIN_HELP: &str = r#"
插件管理

用法:
  caelix plugin <操作> [选项]

操作:
  list          列出所有插件
  info <NAME>   查看插件详情
  -h, --help    显示帮助信息

示例:
  caelix plugin list
  caelix plugin info my_plugin
"#;

pub const SECURITY_HELP: &str = r#"
安全管理

用法:
  caelix security <操作> [选项]

操作:
  config                              显示安全配置
  check path <PATH>                   检查路径是否安全
  check url <URL>                     检查 URL 是否安全
  check command <CMD>                 检查命令是否安全
  add path include <PATH>             添加允许路径
  add path exclude <PATH>             添加排除路径
  add url include <URL>               添加允许 URL
  add url exclude <URL>               添加排除 URL
  add command include <CMD>           添加允许命令
  add command exclude <CMD>           添加排除命令
  -h, --help                          显示帮助信息

示例:
  caelix security config
  caelix security check path /tmp
  caelix security add path include /workspace
"#;

pub const PROVIDER_HELP: &str = r#"
提供商管理

用法:
  caelix provider <操作> [选项]

操作:
  list              列出所有提供商
  models <NAME>     查看提供商的模型列表
  -h, --help        显示帮助信息

示例:
  caelix provider list
  caelix provider models openai
"#;

pub const USAGE_HELP: &str = r#"
Token 用量

用法:
  caelix usage [选项]

选项:
  -s, --session <ID>    查看指定会话的用量
  -g, --global          查看全局用量（默认）
  -h, --help            显示帮助信息

示例:
  caelix usage
  caelix usage -s my-session
  caelix usage --global
"#;

pub const TASK_HELP: &str = r#"
任务管理

用法:
  caelix task <操作> [选项]

操作:
  list [--session <ID>]   列出任务（可选按会话过滤）
  -h, --help               显示帮助信息

示例:
  caelix task list
  caelix task list --session my-session
"#;

pub const MEMORY_HELP: &str = r#"
记忆管理

用法:
  caelix memory <操作> [选项]

操作:
  recall <QUERY> [-k N]           检索记忆（默认返回 5 条）
  write <CONTENT> [选项]           写入 Raw 层记忆
    --source chat|meeting|tweet|paper|note   来源类型（默认 chat）
    --tag TAG                                    添加标签（可重复）
  promote --raw <FILE>            手动触发 Raw→Wiki 晋升
  promote --wiki <ENTITY>         手动触发 Wiki→Axiom 晋升
  flags [--all]                   列出冲突和 Axiom 候选
  rebuild-index                    重建反向索引
  stats                            显示记忆统计信息
  axioms [--include-deprecated]   查看 Axiom 列表
  budget                           查看 LLM 预算使用情况
  -h, --help                       显示帮助信息

示例:
  caelix memory recall "Rust 并发"
  caelix memory write "今天学习了 Rust 的生命周期" --source note --tag rust
  caelix memory stats
"#;

pub const LOGS_HELP: &str = r#"
日志管理

用法:
  caelix logs <操作>

操作:
  dir             显示日志目录路径
  ls              列出所有日志文件
  show [-n N]     显示当前日志最后 N 行 (默认 50)
  follow          实时跟随当前日志 (Ctrl+C 退出)
  clean           删除所有日志文件
  -h, --help      显示帮助信息

示例:
  caelix logs dir
  caelix logs ls
  caelix logs show -n 100
  caelix logs follow
"#;

pub fn get_command_help(cmd: &str) -> &'static str {
    match cmd {
        "chat" => CHAT_HELP,
        "tool" => TOOL_HELP,
        "list" => LIST_HELP,
        "session" => SESSION_HELP,
        "variable" => VARIABLE_HELP,
        "agent" => AGENT_HELP,
        "skill" => SKILL_HELP,
        "command" => COMMAND_HELP,
        "hook" => HOOK_HELP,
        "plugin" => PLUGIN_HELP,
        "security" => SECURITY_HELP,
        "provider" => PROVIDER_HELP,
        "usage" => USAGE_HELP,
        "task" => TASK_HELP,
        "memory" => MEMORY_HELP,
        "logs" => LOGS_HELP,
        _ => CLI_HELP,
    }
}
