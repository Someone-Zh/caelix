use clap::{Subcommand, Args};

use crate::doc;

#[derive(Debug, Subcommand)]
pub enum CaelixCommand {
    #[command(about = "对话聊天")]
    Chat(ChatArgs),

    #[command(about = "工具执行")]
    Tool(ToolArgs),

    #[command(about = "列表查询")]
    List(ListArgs),

    #[command(about = "会话管理")]
    Session(SessionArgs),

    #[command(about = "变量管理")]
    Variable(VariableArgs),

    #[command(about = "智能体管理")]
    Agent(AgentArgs),

    #[command(about = "技能管理")]
    Skill(SkillArgs),

    #[command(about = "命令管理")]
    Command(CommandArgs),

    #[command(about = "Hook 管理")]
    Hook(HookArgs),

    #[command(about = "插件管理")]
    Plugin(PluginArgs),

    #[command(about = "安全管理")]
    Security(SecurityArgs),

    #[command(about = "提供商管理")]
    Provider(ProviderArgs),

    #[command(about = "Token 用量")]
    Usage(UsageArgs),

    #[command(about = "任务管理")]
    Task(TaskArgs),

    #[command(about = "记忆管理")]
    Memory(MemoryArgs),
}

#[derive(Debug, Args)]
#[command(after_help = doc::CHAT_HELP)]
pub struct ChatArgs {
    #[arg(short = 's', long = "session", help = "指定会话 ID（未提供则自动创建）")]
    pub session_id: Option<String>,

    #[arg(short = 'a', long = "agent", help = "指定使用的 Agent")]
    pub agent: Option<String>,

    #[arg(short = 'p', long = "provider", help = "指定提供商")]
    pub provider: Option<String>,

    #[arg(short = 'm', long = "model", help = "指定模型")]
    pub model: Option<String>,

    #[arg(short = 'c', long = "content", help = "对话内容（必需）")]
    pub content: Option<String>,

}

#[derive(Debug, Args)]
#[command(after_help = doc::TOOL_HELP)]
pub struct ToolArgs {
    #[command(subcommand)]
    pub action: Option<ToolAction>,

}

#[derive(Debug, Subcommand)]
pub enum ToolAction {
    #[command(about = "执行指定工具")]
    Exec {
        tool_name: String,
        #[arg(long = "args", help = "以 JSON 格式传递工具参数")]
        args_json: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    #[command(about = "列出所有可用工具")]
    List,
    #[command(about = "查看工具详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::LIST_HELP)]
pub struct ListArgs {
    #[arg(help = "查询类型: sessions|agents|tools|skills|commands|hooks|plugins|providers")]
    pub list_type: Option<String>,

}

#[derive(Debug, Args)]
#[command(after_help = doc::SESSION_HELP)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub action: Option<SessionAction>,

}

#[derive(Debug, Subcommand)]
pub enum SessionAction {
    #[command(about = "列出所有会话")]
    List,
    #[command(about = "查看会话详情（消息历史）")]
    Info {
        session_id: String,
    },
    #[command(about = "创建新会话（可选指定 ID）")]
    Create {
        session_id: Option<String>,
    },
    #[command(about = "删除会话")]
    Delete {
        session_id: String,
    },
    #[command(about = "停止会话中运行的 Agent")]
    Stop {
        session_id: String,
    },
    #[command(about = "设置会话的提供商")]
    SetProvider {
        session_id: String,
        provider: String,
    },
    #[command(about = "设置会话的模型")]
    SetModel {
        session_id: String,
        model: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::VARIABLE_HELP)]
pub struct VariableArgs {
    #[command(subcommand)]
    pub action: Option<VariableAction>,

}

#[derive(Debug, Subcommand)]
pub enum VariableAction {
    #[command(about = "列出所有全局变量")]
    List,
    #[command(about = "获取变量值")]
    Get {
        key: String,
    },
    #[command(about = "设置变量")]
    Set {
        key: String,
        value: String,
    },
    #[command(about = "删除变量")]
    Delete {
        key: String,
    },
    #[command(about = "空间变量管理")]
    Space {
        space: String,
        #[command(subcommand)]
        action: SpaceVariableAction,
    },
    #[command(about = "替换文本中的变量")]
    Replace {
        text: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum SpaceVariableAction {
    #[command(about = "列出空间变量")]
    List,
    #[command(about = "获取空间变量值")]
    Get {
        key: String,
    },
    #[command(about = "设置空间变量")]
    Set {
        key: String,
        value: String,
    },
    #[command(about = "删除空间变量")]
    Delete {
        key: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::AGENT_HELP)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub action: Option<AgentAction>,

}

#[derive(Debug, Subcommand)]
pub enum AgentAction {
    #[command(about = "列出所有智能体")]
    List,
    #[command(about = "查看智能体详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::SKILL_HELP)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub action: Option<SkillAction>,

}

#[derive(Debug, Subcommand)]
pub enum SkillAction {
    #[command(about = "列出所有技能")]
    List,
    #[command(about = "查看技能详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::COMMAND_HELP)]
pub struct CommandArgs {
    #[command(subcommand)]
    pub action: Option<CommandAction>,

}

#[derive(Debug, Subcommand)]
pub enum CommandAction {
    #[command(about = "列出所有命令")]
    List,
    #[command(about = "查看命令详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::HOOK_HELP)]
pub struct HookArgs {
    #[command(subcommand)]
    pub action: Option<HookAction>,

}

#[derive(Debug, Subcommand)]
pub enum HookAction {
    #[command(about = "列出所有 Hook")]
    List,
    #[command(about = "查看 Hook 详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::PLUGIN_HELP)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub action: Option<PluginAction>,

}

#[derive(Debug, Subcommand)]
pub enum PluginAction {
    #[command(about = "列出所有插件")]
    List,
    #[command(about = "查看插件详情")]
    Info {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::SECURITY_HELP)]
pub struct SecurityArgs {
    #[command(subcommand)]
    pub action: Option<SecurityAction>,

}

#[derive(Debug, Subcommand)]
pub enum SecurityAction {
    #[command(about = "显示安全配置")]
    Config,
    #[command(about = "检查路径/URL/命令是否安全")]
    Check {
        #[command(subcommand)]
        target: SecurityCheckTarget,
    },
    #[command(about = "添加安全规则")]
    Add {
        #[command(subcommand)]
        rule: SecurityAddRule,
    },
}

#[derive(Debug, Subcommand)]
pub enum SecurityCheckTarget {
    #[command(about = "检查路径")]
    Path { path: String },
    #[command(about = "检查 URL")]
    Url { url: String },
    #[command(about = "检查命令")]
    Command { command: String },
}

#[derive(Debug, Subcommand)]
pub enum SecurityAddRule {
    #[command(about = "添加路径规则")]
    Path {
        #[command(subcommand)]
        action: IncludeExclude,
    },
    #[command(about = "添加 URL 规则")]
    Url {
        #[command(subcommand)]
        action: IncludeExclude,
    },
    #[command(about = "添加命令规则")]
    Command {
        #[command(subcommand)]
        action: IncludeExclude,
    },
}

#[derive(Debug, Subcommand)]
pub enum IncludeExclude {
    #[command(about = "添加允许规则")]
    Include { value: String },
    #[command(about = "添加排除规则")]
    Exclude { value: String },
}

#[derive(Debug, Args)]
#[command(after_help = doc::PROVIDER_HELP)]
pub struct ProviderArgs {
    #[command(subcommand)]
    pub action: Option<ProviderAction>,

}

#[derive(Debug, Subcommand)]
pub enum ProviderAction {
    #[command(about = "列出所有提供商")]
    List,
    #[command(about = "查看提供商的模型列表")]
    Models {
        name: String,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::USAGE_HELP)]
pub struct UsageArgs {
    #[arg(short = 's', long = "session", help = "查看指定会话的用量")]
    pub session_id: Option<String>,

    #[arg(short = 'g', long = "global", help = "查看全局用量（默认）")]
    pub global: bool,

}

#[derive(Debug, Args)]
#[command(after_help = doc::TASK_HELP)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub action: Option<TaskAction>,

}

#[derive(Debug, Subcommand)]
pub enum TaskAction {
    #[command(about = "列出任务（可选按会话过滤）")]
    List {
        #[arg(long = "session", help = "按会话过滤")]
        session_id: Option<String>,
    },
}

#[derive(Debug, Args)]
#[command(after_help = doc::MEMORY_HELP)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub action: Option<MemoryAction>,

}

#[derive(Debug, Subcommand)]
pub enum MemoryAction {
    #[command(about = "检索记忆")]
    Recall {
        query: String,
        #[arg(short = 'k', long = "top-k", default_value_t = 5, help = "返回结果数量")]
        top_k: u32,
    },
    #[command(about = "写入 Raw 层记忆")]
    Write {
        content: String,
        #[arg(long = "source", default_value = "chat", help = "来源类型: chat|meeting|tweet|paper|note")]
        source: String,
        #[arg(long = "tag", help = "添加标签（可重复）")]
        tags: Vec<String>,
    },
    #[command(about = "手动触发晋升")]
    Promote {
        #[arg(long = "raw", help = "触发 Raw→Wiki 晋升，指定文件")]
        raw: Option<String>,
        #[arg(long = "wiki", help = "触发 Wiki→Axiom 晋升，指定实体")]
        wiki: Option<String>,
    },
    #[command(about = "列出冲突和 Axiom 候选")]
    Flags {
        #[arg(long = "all", help = "显示所有（包括已处理的）")]
        all: bool,
    },
    #[command(about = "重建反向索引")]
    RebuildIndex,
    #[command(about = "显示记忆统计信息")]
    Stats,
    #[command(about = "查看 Axiom 列表")]
    Axioms {
        #[arg(long = "include-deprecated", help = "包含已废弃的 Axiom")]
        include_deprecated: bool,
    },
    #[command(about = "查看 LLM 预算使用情况")]
    Budget,
}

pub fn print_help_for(cmd: &str) {
    println!("{}", doc::get_command_help(cmd));
}
