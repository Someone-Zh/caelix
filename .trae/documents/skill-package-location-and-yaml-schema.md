# 计划：技能包支持 —— Skill 位置记录与 YAML schema 扩展（基础层）

## 概述

为 `Skill` 数据模型增加「来源位置」记录能力，并扩展 `.skill` 文件的 YAML 头以支持声明工具列表（`requires_tools`）与技能内脚本（`inline_tools`）等元数据。

本计划只覆盖**基础数据层**：让 `Skill` 在加载时记住自己来自哪个文件、并能携带 Todo.md §4.2.1 设计的全部 YAML 元数据。这样后续执行层（`SkillHook` 恢复、`InlineScriptTool`、`GetSkillDetailTool`）才有办法解析技能内脚本/资源的相对路径——这正是用户强调的核心诉求：

> Skill 加载时需要记录当前 skill 的位置，否则当执行 skill 的资源或脚本时无法了解其位置。

执行层（钩子注入、脚本执行、CLI/HTTP API）不在本次范围。

---

## 现状分析（基于 Phase 1 探索）

### 当前 `Skill` 结构 — `caelix-api/src/managers/skill.rs:9-21`
```rust
pub struct Skill {
    pub name: String,
    pub namespace: String,
    pub full_name: String,
    pub description: String,
    pub content: String,
}
```
- **没有任何路径/位置字段。** 一旦 `Skill::new` 构造完成，源文件信息就丢失。
- `Skill::new(name, namespace, description, content)`（line 25-39）4 参数，内部派生 `full_name`。

### 当前 YAML schema — `caelix-config/src/skills_loader.rs:36-42`
```rust
struct SkillConfig {
    #[allow(dead_code)] name: String,   // 解析了但未使用（名字取自文件名）
    description: String,
}
```
- 仅解析 `name`、`description`，**不解析** `version`/`tags`/`requires_tools`/`inline_tools`/`script`。
- Todo.md §4.2.1（line 120-140）已给出目标 YAML 格式但**完全未实现**。

### 加载器 — `caelix-config/src/skills_loader.rs`
- `load_single_skill(file_path, base_dir)`（line 96-123）：**手握** `file_path` 和 `base_dir`，只用它们计算 `namespace`，随后丢弃——没有传入 `Skill`。
- 仓库内**没有任何 `.skill` 示例文件**（glob `**/*.skill` 为空）。

### 并行 DTO `SkillDef` — `caelix-api/src/plugins.rs:18-40`
- 与 `Skill` 完全平行的 4 字段，用于跨插件边界传递。
- `Skill ↔ SkillDef` 转换发生在两处：
  1. `caelix-service/src/plugins.rs:80-85`（`Skill → SkillDef`，全局加载路径）
  2. `caelix-runtime/src/context/mod.rs:340-345`（`SkillDef → Skill`，init 第 4 步重建）

### 第三条加载路径（项目级覆盖）
- `caelix-runtime/src/context/mod.rs:91-101`：`ConfigOverlay::ensure_loaded` 直接调用 `load_skills_from_directory` 并把 `Skill` 插入 `ProjectConfig.skills`，**不经过 `SkillDef`**。此路径会自动受益于 loader 的改动，无需单独处理。

### 关键约束
- `caelix-config` 依赖 `caelix-api`（通过 `caelix-config/src/managers/mod.rs` 的 `pub use caelix_api::managers::*`）。因此新类型 `InlineToolDef` 应定义在 `caelix-api`，`caelix-config` 自动可见。
- `caelix-api/Cargo.toml` 已有 `serde`（无需新增依赖）。`serde_yaml` 只在 `caelix-config` 用作反序列化引擎，配合任何 `Deserialize` 类型工作。

---

## 提议的修改

### 1. `caelix-api/src/managers/skill.rs` — 扩展 `Skill` 与新增 `InlineToolDef`

**Why**: 这是基础数据模型。所有下游（loader、DTO、覆盖层、未来的执行层）都从这里取数据。

**What**:

新增 `InlineToolDef`（技能内脚本工具定义，对应 YAML `inline_tools` 的一条）：
```rust
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InlineToolDef {
    pub name: String,
    pub description: String,
    pub script: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}
```

扩展 `Skill` 结构（新增 6 个 pub 字段，沿用现有「扁平 pub 字段」约定）：
```rust
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub namespace: String,
    pub full_name: String,
    pub description: String,
    pub content: String,
    /// .skill 文件的绝对路径；技能内脚本/资源以此为基准解析（file_path.parent() 即技能目录）
    pub file_path: PathBuf,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 本技能希望 Agent 拥有的全局工具名（从系统工具池中选取）
    #[serde(default)]
    pub requires_tools: Vec<String>,
    /// 本技能自带的本地脚本工具定义
    #[serde(default)]
    pub inline_tools: Vec<InlineToolDef>,
}
```

调整构造函数：
- 修改 `Skill::new` 签名，**新增 `file_path: PathBuf` 作为第 5 个必填参数**（强制在构造时记录位置，落实用户核心诉求）。其余元数据字段填默认值：
  ```rust
  pub fn new(
      name: String,
      namespace: String,
      description: String,
      content: String,
      file_path: PathBuf,
  ) -> Self { ... }
  ```
- 新增便捷构造函数 `with_metadata`，供 loader 一次性灌入全部 YAML 元数据，避免到处 `mut skill`：
  ```rust
  pub fn with_metadata(
      name: String, namespace: String, description: String, content: String,
      file_path: PathBuf,
      version: Option<String>, author: Option<String>,
      tags: Vec<String>, requires_tools: Vec<String>, inline_tools: Vec<InlineToolDef>,
  ) -> Self { ... }
  ```
  （两个构造函数内部共用一个私有 `build` 函数派生 `full_name`，避免重复。）

### 2. `caelix-api/src/plugins.rs` — 镜像扩展 `SkillDef`

**Why**: `SkillDef` 是跨插件边界的传输对象，必须与 `Skill` 字段对齐，否则元数据在 `Skill → SkillDef → Skill` 往返中丢失。

**What**:
- 给 `SkillDef` 增加与 `Skill` 相同的 6 个新字段（`file_path`、`version`、`author`、`tags`、`requires_tools`、`inline_tools`）。
- `SkillDef::new` 同步增加 `file_path` 必填参数（保持与 `Skill::new` 对称）。
- 新增 `SkillDef::with_metadata`（与 `Skill` 对称）。
- 推荐增加 `From<Skill> for SkillDef` 与 `From<SkillDef> for Skill` 两个实现，集中管理 11 个字段的往返映射，替代 `plugins.rs:80-85` 与 `context/mod.rs:340-345` 两处手写逐字段拷贝。这是对一处已存在、一处新增的重复转换的合理抽象，且未来加字段不会漏改。

### 3. `caelix-config/src/skills_loader.rs` — 扩展 `SkillConfig` 与 `load_single_skill`

**Why**: 解析新 YAML 字段并把 `file_path` 注入 `Skill`。

**What**:

扩展 `SkillConfig`（增加 `serde(default)` 以保持对旧 `.skill` 文件的向后兼容）：
```rust
#[derive(Debug, Deserialize)]
struct SkillConfig {
    #[allow(dead_code)]
    name: String,
    description: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    requires_tools: Vec<String>,
    #[serde(default)]
    inline_tools: Vec<caelix_api::managers::InlineToolDef>,
}
```

修改 `load_single_skill`（line 96-123）：
- 把 `file_path` 转为绝对路径（`std::fs::canonicalize` 或手动 join current_dir），存入 `PathBuf`。
- 调用 `Skill::with_metadata(...)` 一次性传入 `file_path` 与解析出的元数据，替代当前的 `Skill::new(name, namespace, config.description, skill_content)`。
- 注意：`file_path` 已经在函数参数里（`file_path: &Path`），只需 `.to_path_buf()` 并确保绝对化。

`register_all_skills`（line 126-141，当前为死代码）无需改动——它只做 `register` 转发。

### 4. `caelix-service/src/plugins.rs:66-88` — 更新 `Skill → SkillDef` 转换

**Why**: 全局加载路径要把新字段带过插件边界。

**What**:
- 将 `SkillDef::new(skill.name, skill.namespace, skill.description, skill.content)`（line 80-85）替换为 `SkillDef::from(skill)`（若采纳 `From` 实现），或显式传入 6 个新字段。
- 保留对 `skills_dir` 的处理不变。

### 5. `caelix-runtime/src/context/mod.rs:337-350` — 更新 `SkillDef → Skill` 重建

**Why**: init 第 4 步从 `SkillDef` 重建 `Skill` 时要把新字段带回。

**What**:
- 将 `Skill::new(skill_def.name, skill_def.namespace, skill_def.description, skill_def.content)`（line 340-345）替换为 `Skill::from(skill_def)`（若采纳 `From` 实现），或显式传入新字段。
- `ConfigOverlay::ensure_loaded`（line 91-101）**无需改动**——它直接持有 `Skill`，loader 的新字段自动随对象流入 `ProjectConfig.skills`。

### 6. （可选）`caelix-api/src/managers/mod.rs` — 导出 `InlineToolDef`

确认 `pub mod skill;` 后 `InlineToolDef` 通过 `pub use` 链路对 `caelix-config` 可见。若 `managers/mod.rs` 使用 `pub use skill::*;` 则自动导出；否则显式加 `pub use skill::InlineToolDef;`。

---

## 假设与决策

1. **范围决策**：本次只做基础数据层（位置记录 + YAML 解析 + DTO 镜像）。执行层（`SkillHook::on_init` 恢复、`InlineScriptTool`、`GetSkillDetailTool`、CLI/HTTP API）留给后续迭代。依据：用户只点名了 `skill.rs` 与 `skills_loader.rs` 两个数据模型文件，且核心诉求是「加载时记录位置」。

2. **位置字段形状**：只用单个 `file_path: PathBuf`（绝对路径）记录位置。技能目录 = `file_path.parent()`。不额外存 `base_dir`——`base_dir` 仅用于计算 `namespace`，计算完即无需保留；未来执行层若需 resources 根目录，可从 `file_path` 派生。这避免冗余字段，符合「最小复杂度」。

3. **脚本/资源路径解析**（为后续执行层预留约定，本次不实现）：`inline_tools[].script` 以 `file_path.parent()` 为 CWD 执行；任何相对路径资源相对该目录解析。这正是记录 `file_path` 的用途。

4. **`file_path` 必填**：放进 `Skill::new` 必填参数而非 `Option`，强制所有构造点记录位置。3 个调用点（loader、plugins.rs、context/mod.rs）都已有路径信息，可无缝满足。

5. **向后兼容**：`SkillConfig` 所有新字段用 `#[serde(default)]`，旧 `.skill` 文件（只有 `name`+`description`）继续正常加载。

6. **`From` 转换抽象**：采纳 `From<Skill> for SkillDef` + `From<SkillDef> for Skill`。理由：11 字段往返、2 处调用、且 `SkillDef` 本就是 `Skill` 的传输副本，`From` 是惯用且非过度的抽象。

7. **`InlineToolDef` 位置**：定义在 `caelix-api/src/managers/skill.rs`（与 `Skill` 同居），`caelix-config` 经 re-export 直接复用，避免在 `caelix-config` 重复定义 `InlineToolConfig`。

---

## 验证步骤

1. **编译**：`cargo build -p caelix-api -p caelix-config -p caelix-service -p caelix-runtime` 全部通过，无 warning（特别是 `dead_code`）。
2. **全量构建**：`cargo build --workspace` 通过。
3. **单元测试**（新增，放在 `caelix-config/src/skills_loader.rs` 模块测试区）：
   - 用 `tempfile::tempdir()` 造一个临时 skills 目录，写入：
     - 一个仅含 `name`+`description` 的旧式 `.skill` 文件 → 断言 `file_path` 正确、`inline_tools`/`requires_tools`/`tags` 为空、`version`/`author` 为 `None`。
     - 一个含全部新字段（`requires_tools`、`inline_tools`、`version`、`tags`、`author`）的 `.skill` 文件 → 断言所有字段被正确解析、`file_path` 为绝对路径、`inline_tools[0].script` 内容正确。
   - 若 `caelix-config` 未引入 `tempfile` 依赖，先在 `Cargo.toml` `[dev-dependencies]` 添加。
4. **往返一致性测试**：构造 `Skill` → `SkillDef::from` → `Skill::from` → 与原 `Skill` 逐字段相等（验证 `From` 实现无字段遗漏）。
5. **回归**：现有 `caelix-config` 与 `caelix-runtime` 测试 `cargo test -p caelix-config -p caelix-runtime` 全绿。
6. **手动抽样**：在 `$CAELIX_HOME/skills/` 放一个含 `inline_tools` 的示例 `.skill` 文件，启动应用，观察日志 `Loading skill` 行正常输出且无加载错误。
