# Caelix 待实现功能设计文档

本文档基于对 Caelix 现有代码架构（`caelix-api` / `caelix-llm` / `caelix-message` / `caelix-config` / `caelix-runtime` / `caelix-tools` / `caelix-task` / `caelix-agent` / `caelix-service` / `caelix-cli` / `caelix-http` / `caelix-tui` / `caelix-bin` / `caelix-security`）的全面分析，详细设计如下六大功能的实现方案。

---

## 1. 上下文窗口记录 + 用量统计（各维度 token 使用）

### 1.1 现状分析

- **LLM 层**：`OpenAIProvider` 在 `openai.rs` 中发送请求时，仅通过 `ChatResponseChunk` 返回流式内容，**未解析和传递 `usage` 字段**。
- **消息层**：`AgentMessage` 在 `message/mod.rs` 中仅携带 `content` / `timestamp` / `trace_id`，**缺少 token 用量字段**。
- **RuntimeContext**（`context/mod.rs`）：仅维护 `session/request/span/trace` ID，**无上下文窗口大小约束与累计用量跟踪**。
- **统计维度**：目前完全没有对 `prompt_tokens / completion_tokens / total_tokens / cache_hit_tokens` 进行任何累计与上报。

### 1.2 设计方案

#### 1.2.1 LLM Provider 层扩展 — 解析 usage

**文件**：`caelix-api/src/provider/mod.rs`

- 新增 `TokenUsage` 结构体：
  ```rust
  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct TokenUsage {
      pub prompt_tokens: u32,
      pub completion_tokens: u32,
      pub total_tokens: u32,
      /// 推理 token（Claude/DeepSeek 等模型有）
      pub reasoning_tokens: Option<u32>,
      /// 缓存命中 token（OpenAI prompt_cache_details）
      pub cache_hit_tokens: Option<u32>,
  }
  ```
- 在 `ChatResponseChunk` 中追加 `usage: Option<TokenUsage>` 字段。
- 在 `ProviderConfig` 中追加 `ctx_window_tokens: Option<u32>`、`max_output_tokens: Option<u32>` 两个可选参数，用于配置上下文窗口大小与最大输出 token。
- `LlmProvider` trait 中新增可选方法：
  ```rust
  async fn last_usage(&self) -> Option<TokenUsage> { None }
  ```

**文件**：`caelix-llm/src/openai.rs`

- `LlmChatRequest` 结构中新增 `stream_options: Option<Value>`，始终发送 `{"include_usage": true}`，以确保在流式响应末尾 chunk 中返回 `usage` 对象。
- `parse_sse_chunk` 中解析顶层 `usage` 字段并写入 `ChatResponseChunk.usage`。
- `OpenAIProvider` 内部用 `Arc<RwLock<TokenUsage>>` 保存最近一次调用的用量，供 `last_usage()` 读取。
- 在 `AgentRunner` 中拿到 `chunk.usage` 后，交给 `UsageTracker`（见下）累计。

#### 1.2.2 用量跟踪器 `UsageTracker`

**文件**：`caelix-runtime/src/usage_tracker.rs`（新文件）

- 以 `(session_id, request_id, span_id, trace_id, provider, model, agent)` 为维度累计 token：
  ```rust
  #[derive(Debug, Clone, Default)]
  pub struct UsageSnapshot {
      pub session_id: String,
      pub request_id: String,
      pub trace_id: String,
      pub provider: String,
      pub model: String,
      pub agent: Option<String>,
      pub prompt_tokens: u32,
      pub completion_tokens: u32,
      pub total_tokens: u32,
      pub reasoning_tokens: u32,
      pub cache_hit_tokens: u32,
      pub timestamp: DateTime<Utc>,
  }
  ```
- 提供：
  - `fn accumulate(&self, session_id, request_id, trace_id, provider, model, agent, usage)` 累加。
  - `fn snapshot_session(&self, session_id) -> UsageSnapshot` 返回整个 session 累计。
  - `fn snapshot_request(&self, request_id) -> UsageSnapshot`。
  - `fn snapshot_global(&self) -> Vec<UsageSnapshot>` 汇总所有。
- 通过 `CaelixContext` 暴露全局可访问实例（类似 `hook_registry`），并在 `ContextProvider` trait 中追加：
  ```rust
  fn usage_tracker(&self) -> &UsageTracker;
  ```

#### 1.2.3 Agent 消息携带 token 用量

**文件**：`caelix-api/src/message/mod.rs`

- `AgentMessage` 结构追加可选字段 `usage: Option<TokenUsage>`，让前端/日志都能拿到本次请求的 token 用量。

#### 1.2.4 CLI / TUI / HTTP 统计入口

**文件**：`caelix-service/src/api_impl.rs`

- 新增 `get_usage(session_id)` / `get_global_usage()`，让 `caelix-cli` 与 `caelix-http` 都能查询。
- `caelix-cli` 中新增子命令：
  - `caelix usage [--session <id>]`：打印 session 维度的累计用量。
  - `caelix usage --global`：打印所有 provider/model 维度累计。
- `caelix-http` 新增接口：`GET /api/usage?session_id=xxx`、`GET /api/usage/global`。

---

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

### 3.1 现状分析

- `EnvConfig` 仅从环境变量读取 `CAELIX_HOME` 与 `CAELIX_DEBUG`，没有项目级（per-directory）配置文件。
- `ProviderConfig` / `AgentSpec` 是全局加载的：一旦启动进程，所有 session 使用同一套 provider/agent，无法在**某个 Git 仓库根目录**下使用不同的 `api_key`、`default_model`、`work_dir`、`skills`、`allow_commands`。
- 目前 `CommandExecTool` 的命令与路径白名单依赖 `caelix-security`，与 provider/agent 配置**不在同一处**，配置体验零散。

### 3.2 设计方案

#### 3.2.1 定义 `ProjectConfig` 结构

**文件**：`caelix-api/src/project/mod.rs`（新模块）

```toml
# ~/my_project/.caelix.toml  （项目根目录文件）

[project]
name = "my_project"
default_agent = "code_executor_agent"   # 覆盖全局默认
default_provider = "openai"
default_model = "gpt-4"

[project.ctx_window]
tokens = 8000        # 上下文窗口大小；超过时走第 2 节的压缩
max_output_tokens = 2000

[project.provider.overrides]
# 只在本项目下替换 base_url / api_key 等字段，不写明文 key 时走环境变量
openai.base_url = "https://xxx.yyy/v1"
openai.api_key_env = "MY_PROJECT_OPENAI_KEY"
openai.temperature = 0.2

[project.security]
allow_commands = ["git", "cargo", "npm"]
allow_paths = ["/home/user/my_project", "/tmp"]

[project.memory]
enable_summary = true
external_skills = ["skills/coding/rust.skill"]
external_files = ["docs/README.md"]

[project.logging]
level = "debug"
```

#### 3.2.2 查找与合并策略

**文件**：`caelix-config/src/project_loader.rs`（新文件）

- 启动时从 `work_dir` 向上遍历到 `$HOME`，查找所有 `.caelix.toml`（允许多层：系统级 `~/.caelix/config.toml` + 项目级）。
- 合并顺序（后面覆盖前面）：`defaults → ~/.caelix/config.toml → /parent/.caelix.toml → /current/.caelix.toml`。
- 合并规则：
  - 标量字段（`default_agent`、`default_model`）：直接覆盖。
  - 数组字段（`allow_commands`、`external_skills`）：并集。
  - `provider.overrides`：对 `ProviderConfig` 做字段级 patch（仅当配置文件中存在时覆盖）。
- 在 `ContextProvider` 中暴露：
  ```rust
  fn project_config(&self) -> &ProjectConfig;
  fn reload_project_config(&self, path: &Path) -> Result<(), String>;
  ```
- `RuntimeContext` 在 `new()` 时读取 `project_config()` 作为默认 `provider` / `model`。

#### 3.2.3 安全性与隔离

- `.caelix.toml` 中所有命令与路径都会被 `SecurityCheckerTrait` 校验；若项目级 `allow_*` 比全局更宽松，默认拒绝并提示（可通过 `CAELIX_ALLOW_UNSAFE_PROJECT=1` 显式启用）。
- `api_key_env` 必须是**环境变量名**，不允许明文写入 key。

---

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



## 7. 实施顺序建议

| 顺序 | 模块 | 理由 |
|------|------|------|
| 1 | **日志系统（第 6 节）** | 最先做完；后续所有模块开发都能享受结构化日志 |
| 2 | **项目级配置（第 3 节）** | 建立 `LogConfig`/`ProjectConfig` 的统一加载管线；后续功能可复用 |
| 3 | **LLM usage 统计（第 1 节）** | 直接在 `openai.rs` + `ChatResponseChunk` 上扩展，不依赖其他功能 |
| 4 | **记忆融合 / 压缩 / 卸载（第 2 节）** | 依赖 LLM usage（token 计数）；依赖日志 |
| 5 | **技能包（第 4 节）** | 需要 `HookRegistry.on_init()` 正常工作；可与第 2 节并行 |
| 6 | **AST 工具（第 5 节）** | 独立工具，最后集成即可 |

---

## 8. 测试与验收清单

- [ ] `cargo build --workspace` 全部通过。
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- [ ] 单次 `caelix "你的提示词"` 后：
  - `~/.caelix/logs/caelix.*.log` 中有若干 JSON 行，包含 session_id。
  - `caelix usage` 能看到 `prompt_tokens / completion_tokens` 不为 0。
- [ ] 连续多轮对话（> 8k tokens）后，`ContextManager` 应触发压缩并返回 `Summary`。
- [ ] `caelix skills list` 能列出本地 skills；`caelix skills show <name>` 能显示内容。
- [ ] `caelix ast list src/agent/loop_agent.rs` 能正确列出所有 `fn` 符号与行号。
- [ ] 新起一个项目目录，在其中写 `.caelix.toml` 后执行 `caelix`，应读取到项目级覆盖配置。
