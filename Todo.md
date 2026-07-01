# Caelix 待实现功能设计文档

## 1. 变量系统


## 2. 记忆融合、压缩、卸载

### 2.1 现状分析

- 历史消息以 **JSON Lines**（`agent_messages.jsonl`）形式持久化在 `FileStorage`（`caelix-message/src/storage.rs`）。每条是完整 `ChatMessage`，**未做任何压缩/摘要**。
- `AgentSpec.build_messages()` 直接将 `system_prompt + user_input` 拼接，**无条件将所有历史消息注入上下文**——长对话极易超出模型上下文窗口。
- 没有“记忆”的概念：无法把多 session 的知识库内容注入、也无法把旧 session 摘要注入。

### 2.2 设计方案

#### 2.2.1 定义 Memory 抽象

**文件**：`caelix-api/src/memory/mod.rs`（新文件 + 新模块）

```rust
use async_trait::async_trait;
use crate::provider::ChatMessage;

/// 单条记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub tokens: u32,
    pub priority: f32,        // 0~1，越大越优先
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub tags: Vec<String>,    // 用于过滤与融合
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryKind {
    System,        // system_prompt / 注入的规则
    Conversation,  // 用户与助手对话
    ToolResult,    // 工具执行结果
    Summary,       // 压缩后的摘要
    External,      // 外部知识库注入
}

/// 记忆后端：可替换实现（文件 / SQLite / vector DB）
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn insert(&self, item: MemoryItem) -> Result<(), String>;
    async fn list_by_session(&self, session_id: &str) -> Result<Vec<MemoryItem>, String>;
    async fn prune(&self, session_id: &str, max_tokens: u32) -> Result<Vec<MemoryItem>, String>;
    async fn clear(&self, session_id: &str) -> Result<(), String>;
}
```

#### 2.2.2 上下文管理器 `ContextManager`（在 `caelix-runtime` 中实现）

**文件**：`caelix-runtime/src/context_manager.rs`（新文件）

负责：
1. **融合**：把 `system_prompt`、session 历史、外部知识库、已压缩 summary 合并为有序列表。
2. **压缩**：当 `total_tokens > ctx_window_tokens` 时，对 `priority` 最低的若干条消息调用 LLM 进行摘要（"把以下 N 条消息压缩为 1 条摘要"），并用一条 `MemoryKind::Summary` 替换原来的多条。
   - 压缩规则：保留最近 K 条消息（priority 最高），把较旧的消息逐条合并为摘要。
   - 摘要内容本身也计入 token，递归压缩直到满足窗口大小。
3. **卸载**：当 session 结束或调用 `--unload <session_id>` 时，把整个 session 的消息（非摘要部分）落盘到 `~/.caelix/memory/<session_id>/archive.jsonl.gz`，并从内存中移除；下次进入 session 时可按需只读 summary 不读原始消息。
4. **暴露 `build_context()`**：返回 `Vec<ChatMessage>`，供 `Agent::run()` 使用，替换原来的 naive `build_messages()`。

在 `ContextProvider` trait 中新增：

```rust
fn memory_store(&self) -> Arc<dyn MemoryStore>;
fn context_manager(&self) -> &ContextManager;
```

#### 2.2.3 Agent 层对接

**文件**：`caelix-agent/src/agent_runner.rs`

- 现有流程：
  ```
  messages = agent_spec.build_messages(user_input)
  → provider.chat_stream(messages, tools, config)
  ```
- 新流程：
  ```
  let messages = ctx.context_manager().build_context(
      session_id, &agent_spec.system_prompt, user_input
  ).await?;
  ```
- 每次 `Finish` 之后，把本次 request 的新 user / assistant / tool 消息写入 `MemoryStore`，并根据配置触发一次压缩检查。

#### 2.2.4 CLI 命令

- `caelix memory --list`：列出当前 session 的记忆与 token 数。
- `caelix memory --summary`：强制对当前 session 做一次压缩。
- `caelix memory --unload <session_id>`：卸载某个 session（仅保留摘要）。
- `caelix memory --load <session_id>`：重新加载被卸载的 session。
- `caelix memory --external <path>`：注入外部 Markdown 作为 `External` 记忆。

---

## 3. 项目级配置融合

## 4. 技能包支持（技能与工具的列表以及技能内脚本）

### 4.1 现状分析

- `SkillManager`（`caelix-api/src/managers/skill.rs`）仅保存 `Skill { name, namespace, description, content }`。
- `skills_loader.rs`（`caelix-config`）递归扫描 `.skill` 文件并注册到 `SkillManager`。
- `SkillHook`（`caelix-runtime/src/hooks/skill_hook.rs`）**目前是空实现**（`// TODO: 恢复技能钩子逻辑，由于循环依赖暂不使用`）。
- 技能内容是 Markdown 文本，**无法携带工具定义与可执行脚本**，也无法与具体 Agent 的 `tools` 列表绑定。
- `GetSkillDetailTool` 缺失：Agent 无法按需查询技能内容。

### 4.2 设计方案

#### 4.2.1 扩展 Skill 格式

`.skill` 文件为 YAML 头 + Markdown 内容格式。扩展 YAML 头新增元数据：

```yaml
---
name: rust_coding
description: Rust 编程与工具链操作
version: "1.0"
author: "team"
tags: ["rust", "cargo"]
# 声明本技能希望 Agent 拥有的工具（从系统工具池中选择）
requires_tools: ["read_file", "write_file", "exec_command"]
# 声明本技能自带的"本地工具脚本"
inline_tools:
  - name: cargo_check
    description: "运行 cargo check"
    script: "cargo check --message-format=short"
    # 脚本运行时的安全参数
    timeout_secs: 60
---

# Rust 编码指南
...
```

#### 4.2.2 新工具 `InlineScriptTool`

**文件**：`caelix-tools/src/inline_script_tool.rs`（新文件）

- 实现 `Tool` trait。
- 接收技能 YAML 中 `inline_tools` 一条定义，当被 Agent 调用时执行其 `script`（通过 `CommandExecTool` 同样的安全检查管线）。
- 每次执行前都经过 `SecurityCheckerTrait::check_command` 与 `check_path`。

#### 4.2.3 恢复 `SkillHook` 逻辑

**文件**：`caelix-runtime/src/hooks/skill_hook.rs`

- 去掉 `Arc<SkillManager>` 的字段声明上的 `#[allow(dead_code)]`。
- `on_init()` 实现：
  1. 读取所有 skill，把 `requires_tools` 从 `ToolManager` 取出并注入到 `agent_spec.tools`（去重）。
  2. 把 `inline_tools` 实例化为 `InlineScriptTool`，注入 `agent_spec.tools`。
  3. 把 `name + description` 合并成"可用技能列表"附加到 `agent_spec.system_prompt` 末尾。
  4. 注入 `get_skill_detail(skill_name)` 工具（见下）。

#### 4.2.4 `GetSkillDetailTool`

**文件**：`caelix-tools/src/get_skill_detail.rs`（新文件）

- `name = "get_skill_detail"`
- `parameters = { "skill_name": string, "namespace": string (可选) }`
- 执行时从 `SkillManager` 读取对应 skill，返回 `content`。
- 依赖 `SkillManager` 通过 `CaelixContext` 注入，避免 `skill_hook.rs` 中之前遇到的循环依赖问题。

#### 4.2.5 技能清单 API 与 CLI

- `caelix-cli` 增加命令：
  - `caelix skills list`：列出所有技能（含命名空间、tags、requires_tools）。
  - `caelix skills show <skill_name>`：打印完整技能内容。
  - `caelix skills reload`：重新扫描 `~/.caelix/skills` 与 `$PROJECT/.caelix/skills`。
- `caelix-http` 增加：`GET /api/skills`、`GET /api/skills/:name`。

---

## 5. AST 能力（tree-sitter）

### 5.1 现状分析

- `caelix-tools` 目前只有 `read_file / write_file / string_replace / tree / command_exec`，**完全没有基于 AST 的代码分析工具**。
- `Cargo.toml`（workspace）中未引入 tree-sitter 依赖。
- Agent 若要查询"这个文件里所有函数名 + 起止行号"，只能通过 `read_file + exec_command("grep -n 'fn '")` 的方式做文本搜索，易误匹配、缺乏结构语义。

### 5.2 设计方案

#### 5.2.1 引入 tree-sitter 依赖

**文件**：`Cargo.toml`（根 workspace）

```toml
[workspace.dependencies]
tree-sitter = "0.25"
tree-sitter-highlight = "0.25"
tree-sitter-rust = { version = "0.24", features = ["language"] }
tree-sitter-javascript = { version = "0.23", features = ["language"] }
tree-sitter-typescript = { version = "0.23", features = ["language"] }
tree-sitter-python = { version = "0.23", features = ["language"] }
tree-sitter-go = { version = "0.23", features = ["language"] }
tree-sitter-c = { version = "0.23", features = ["language"] }
```

按需 feature-gate：在 `caelix-tools/Cargo.toml` 中：

```toml
[dependencies]
tree-sitter.workspace = true
tree-sitter-rust = { workspace = true, optional = true }
tree-sitter-python = { workspace = true, optional = true }
# ...

[features]
default = ["regex", "ast-rust", "ast-python", "ast-ts", "ast-go", "ast-c"]
ast-rust = ["tree-sitter-rust"]
ast-python = ["tree-sitter-python"]
ast-ts = ["tree-sitter-javascript", "tree-sitter-typescript"]
ast-go = ["tree-sitter-go"]
ast-c = ["tree-sitter-c"]
```

#### 5.2.2 AST 工具实现

**文件**：`caelix-tools/src/ast_tool.rs`（新文件）

- 内部维护 `LazyLock<HashMap<String, Language>>`，根据文件扩展名选择语言。
- 对外暴露两个 Tool 接口：

  1. **`list_symbols`**（Tool）
     - 参数：`file_path`（必填）、`kind`（可选：`function / struct / enum / class / method / all`，默认 `all`）
     - 返回：
       ```json
       [
         { "kind": "function", "name": "run_agent", "start_line": 23, "end_line": 169, "signature": "fn run_agent(agent_spec: Arc<AgentSpec>, ...)" },
         ...
       ]
       ```
     - 用 tree-sitter 解析源文件，递归遍历 node，根据语言的节点类型（Rust 的 `function_item`、Python 的 `function_definition`、TS 的 `method_definition` 等）做映射。

  2. **`get_symbol_definition`**（Tool）
     - 参数：`file_path`、`symbol_name`
     - 返回该符号的完整源码片段 + 行号范围，便于 Agent "查看某函数的实现"而无需读整个文件。

#### 5.2.3 注册到 ToolManager

**文件**：`caelix-runtime/src/context/mod.rs`（或相应的初始化流程）

- 在 `init_context` 时，把 `list_symbols` 与 `get_symbol_definition` 两个工具注册到全局 `ToolManager`。
- 所有 Agent 默认可以使用（通过 skill 的 `requires_tools` 中显式声明也可单独授权）。

#### 5.2.4 CLI 入口（可选，但便于调试）

- `caelix ast list <file>`：打印文件的符号清单。
- `caelix ast show <file> <symbol>`：打印指定符号的源码。

---

## 6. 记忆卸载工具




## 8. 测试与验收清单

- [ ] `cargo build --workspace` 全部通过。
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过。

- [ ] `caelix ast list src/agent/loop_agent.rs` 能正确列出所有 `fn` 符号与行号。









为了实现这个轻量化、无数据库的文本记忆系统，我们需要一套明确的**数据规范（Data Schema）**让 LLM 严格遵守，以及一个**系统提示词（System Prompt）**，用于在日常聊天、对话或记录时，指示大模型如何生成或更新这些纯文本记忆。

---

## 一、 数据格式规范（Markdown Schema）

整个系统只有三种轻量化文本文件，存放在一个统一的文件夹中（例如 `Memory_Vault/`）。

### 1. 核心实体文件（高冷节点：人、项目、核心技术）

存放路径：`/Memory_Vault/Entities/{实体名}.md`
**规范：** 保持精简，主要用 YAML 元数据声明属性和别名，**不写**复杂的动态关系链路。

```markdown
---
type: 实体
category: 人员 # 可选: 人员, 项目, 核心技术, 组织
aliases: [老张, 张总, 张大仙]
tags: [财务部, 主管]
---
# 张三

## 核心简介
公司财务部总监，主要负责审批 50 万以上的项目预算。

## 关键信息
* 联系方式: zhansan@company.com
* 办公地点: A栋402

```

### 2. 流水账日志/对话历史（底层容器：日常、聊天、AI对话）

存放路径：`/Memory_Vault/Daily/2026-06-23.md`
**规范：** 纯自然的段落叙述。**短生命周期向高生命周期单向连线**。在提到实体时，使用 `[[实体名]]` 语法（必须使用标准名，不能用别名）。

```markdown
---
type: 日志
date: 2026-06-23
tags: [日常记录, 会议]
---
# 2026-06-23 日志与对话

## 14:00 与 AI 的对话
用户询问了关于轻量化记忆系统的架构。AI 建议脱离数据库，采用纯文本的 Markdown 方案，并使用动态反向索引来防止全连接风暴。

## 15:30 团队同步会
今天下午在探讨 [[Project_Alpha]] 的时候，[[张三]] 提出了一个新的跨境支付需求。[[张三]] 明确表示这个需求的预算需要重新评估，预计会追加 20 万。

## 临时备忘
记得明天把 [[Project_Alpha]] 的架构图发给 [[李四]] 看一下。

```

### 3. 全局别名映射表（极轻量索引）

存放路径：`/Memory_Vault/aliases.json`
**规范：** 扁平的键值对，用于在检索前，将用户的口语化称呼瞬间对齐到标准文件名。

```json
{
  "老张": "张三",
  "张总": "张三",
  "Alpha项目": "Project_Alpha",
  "Alpha Plan": "Project_Alpha",
  "老李": "李四"
}

```

---

## 二、 系统提示词（System Prompt）

当你作为个人助理与用户聊天，或者你在往系统里输入杂乱信息、需要 AI 帮你整理成上述格式时，使用以下提示词。

```markdown
# Role
你是一个高度专业、严谨的“轻量化纯文本记忆网络”构建专家。你的任务是分析用户的输入（包括日常记录、聊天记录、AI对话），并将其转化为结构化的、无数据库依赖的 Markdown 记忆块。

# Core Principles (防爆盾原则)
1. 单向挂靠：日常日志、事件属于“低层级线索”，核心实体（如：张三、Project_Alpha）属于“高层级节点”。只能由低层级文件向高层级节点单向连线（使用 `[[标准实体名]]`）。
2. 禁止平级互联：绝对不要在 `张三.md` 里写他负责什么项目，也不要在 `Project_Alpha.md` 里写谁是负责人。关系必须记录在“事件/日志段落”或“实体的元数据属性”中！
3. 降维打击：诸如“需求A”、“Bug #102”这种细碎的临时线索，不配拥有独立的 `.md` 文件。请将它们作为“上下文”直接写在所属项目的 Markdown 文件中，或者写在当日日志中。

# Task 1: 别名对齐与实体提取
当用户提及某人或某事时：
- 检查该名称是否有别名。如果提及“老张”，自动转换为标准实体名 `[[张三]]`。
- 如果发现新别名（例如用户说：“张大仙今天说...”且确认是张三），请输出更新别名表的 JSON 指令。

# Task 2: 记忆写入格式规范
请根据用户输入，输出以下格式的修改或新增建议：

### 格式 A：若属于日常流水/对话，输出为【日志增量】：
```markdown
## [精准的时间戳或主题]
包含标准双向链接的自然段落描述。
示例：在讨论 [[Project_Alpha]] 时，[[张三]] 提出了关于预算的异议。

```

### 格式 B：若发现新的核心实体，或实体长期属性变更，输出为【实体更新】：

```markdown
文件路径：/Memory_Vault/Entities/标准实体名.md
---
type: 实体
category: [人员/项目/技术/组织]
aliases: [别名1, 别名2]
tags: [标签1]
---
# 标准实体名
## 核心简介
[高度概括的客观描述，严禁写入频繁变动的事件链路]

```

# Execution (输入开始)

请分析以下输入，并严格按照上述防爆盾原则与格式规范，输出对应的纯文本记忆更新建议：

```

---

## 三、 完整运转闭环流向

当你有了这两样东西，你的 Python 检索脚本逻辑就会异常简单：

1. **写输入：** 
   你：“今天张总让我跟进一下 Alpha项目的进度。”
   LLM 根据提示词输出：`今天 [[张三]] 让我跟进一下 [[Project_Alpha]] 的进度。` -> 写入 `2026-06-23.md`。
2. **读检索：**
   你：“张三最近安排了什么工作？”
   脚本处理：
   * **第一步：** 别名表对齐，确认找 `张三`。
   * **第二步：** 物理搜索（Ripgrep / Python `in` 关键字），全局扫描所有 `.md` 文件，找出包含 `[[张三]]` 字符串的**最新 5 个段落**。
   * **第三步：** 抓到了 `2026-06-23.md` 里的那句“今天 [[张三]] 让我跟进...”。
   * **第四步：** 丢给大模型总结，返回给你。完全没有多余的扩散，完美防御全连接风暴。

```
