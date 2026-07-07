# Memory Vault 记忆系统功能规范

## 功能概述

Memory Vault 是 Caelix 的三层架构记忆系统，实现「原始记忆 → 精炼知识 → 核心公理」的渐进式知识沉淀。基于纯文本 + Markdown + 双向链接 + 物理反向索引的设计，提供轻量、可解释、可人工干预的记忆管理能力。

**核心定位**:
- 短期记忆（Raw）：快速记录、噪声较大、按天归档
- 中期记忆（Wiki）：结构化实体、事件定义、双向链接网络
- 长期记忆（Axiom）：经过验证的真理、指导决策的核心原则

---

## 涉及的模型文件

| 描述 | 位置 |
|---|---|
| 数据结构定义（各层 schema、配置、冲突、片段） | [caelix-memory/src/schema.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/schema.rs) |
| 核心 vault 实现（MemoryVault、RecallResult、统计信息） | [caelix-memory/src/vault.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/vault.rs) |
| Raw 层（按天归档的原始记忆） | [caelix-memory/src/raw.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/raw.rs) |
| Wiki 实体层 | [caelix-memory/src/wiki/entity.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/wiki/entity.rs) |
| Wiki 事件层 | [caelix-memory/src/wiki/event.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/wiki/event.rs) |
| Axiom 公理层 | [caelix-memory/src/axiom.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/axiom.rs) |
| 双向链接解析与验证 | [caelix-memory/src/link.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/link.rs) |
| 反向索引管理器 | [caelix-memory/src/index.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/index.rs) |
| 别名管理器 | [caelix-memory/src/alias.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/alias.rs) |
| 冲突与候选管理 | [caelix-memory/src/conflict.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/conflict.rs) |
| LLM 预算管理器 | [caelix-memory/src/budget.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/budget.rs) |
| 晋升引擎（Raw→Wiki、Wiki→Axiom） | [caelix-memory/src/promote.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/promote.rs) |
| 晋升后台 Worker | [caelix-memory/src/promote_worker.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/promote_worker.rs) |
| 记忆压缩 Hook | [caelix-memory/src/compactor_hook.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/compactor_hook.rs) |

---

## 涉及的工具文件

| 描述 | 位置 |
|---|---|
| 记忆写入工具 | [caelix-memory/src/tools/write.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/tools/write.rs) |
| 记忆检索工具 | [caelix-memory/src/tools/recall.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/tools/recall.rs) |
| 记忆晋升工具 | [caelix-memory/src/tools/promote.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/tools/promote.rs) |
| 实体重命名工具 | [caelix-memory/src/tools/rename.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/tools/rename.rs) |
| 冲突/标记管理工具 | [caelix-memory/src/tools/flag.rs](file:///g:/CodeSpace/Rust/caelix/caelix-memory/src/tools/flag.rs) |

---

## 涉及的依赖功能

* [任务调度系统](file:///.spec/caelix-task/spec.md) - 晋升任务提交与后台执行
* [Hook 系统](file:///.spec/caelix-runtime/spec.md) - 记忆压缩钩子、事件触发
* [工具系统](file:///.spec/caelix-tools/spec.md) - Memory 系列工具注册与调用
* [配置管理](file:///.spec/caelix-config/spec.md) - 记忆系统配置加载（待集成）

---

## 三层架构设计

### 1. Raw 层（原始记忆）

**业务逻辑**:
- 按天归档，每天一个 Markdown 文件：`Raw/YYYY-MM-DD.md`
- 每条记忆以 `## HH:MM` 时间戳为标题
- 支持来源标记（chat/meeting/tweet/paper/note）和标签
- 不要求结构化，想到什么写什么
- 自动检测实体提及，作为晋升触发条件

**Frontmatter 格式**:
```yaml
---
source: chat
tags: [项目, 讨论]
created_at: 2026-07-08T10:30:00Z
---
```

**功能实际文件位置**: `caelix-memory/src/raw.rs`

---

### 2. Wiki 层（精炼知识）

**业务逻辑**:
- 分为实体（Entities）和事件（Events）两类
- 实体：人物、地点、概念、项目等，一个实体一个文件
- 事件：会议、发布、事故等，支持多日期范围
- 必须有 frontmatter（category、status、confidence、derived_from）
- 使用 `[[实体名]]` 双向链接建立知识网络
- 置信度（confidence）和溯源（derived_from）用于晋升判断

**Wiki 实体 Frontmatter**:
```yaml
---
category: person | place | concept | project | other
status: active | deprecated
aliases: [别名1, 别名2]
tags: [标签1, 标签2]
confidence: 0.85
derived_from: [Raw/2026-07-08.md#10:30, ...]
created_at: 2026-07-08T10:30:00Z
updated_at: 2026-07-08T10:30:00Z
---
```

**Wiki 事件 Frontmatter**:
```yaml
---
date_range: [2026-07-08, 2026-07-09]
status: active | deprecated
participants: [[张三], [李四]]
related_entities: [[项目A], [项目B]]
confidence: 0.9
derived_from: [Raw/2026-07-08.md#14:00]
created_at: 2026-07-08T10:30:00Z
---
```

**功能实际文件位置**:
- 实体：`caelix-memory/src/wiki/entity.rs`
- 事件：`caelix-memory/src/wiki/event.rs`

---

### 3. Axiom 层（核心公理）

**业务逻辑**:
- 按类别分目录存储：`Axioms/rules/`、`Axioms/principles/`、`Axioms/methodology/`
- 内容是经过验证的真理/原则，用于指导决策
- 置信度要求高，需要人工确认晋升
- 支持废弃（deprecate）和矛盾标记（contradictions）

**Axiom Frontmatter**:
```yaml
---
category: rule | principle | methodology
status: active | deprecated
confidence: 0.95
derived_from: [Wiki/Entities/张三.md, ...]
contradictions: [其他公理名]
created_at: 2026-07-08T10:30:00Z
deprecated_reason: 废弃原因（status=deprecated 时）
---
```

**功能实际文件位置**: `caelix-memory/src/axiom.rs`

---

## 双向链接系统

### 链接语法

| 格式 | 含义 | 示例 |
|---|---|---|
| `[[名称]]` | 标准实体链接 | `[[张三]]` |
| `[[Event:名称]]` | 事件链接 | `[[Event:项目启动会]]` |
| `[[Axiom:名称]]` | 公理链接 | `[[Axiom:二八定律]]` |
| `[[名称?]]` | 待确认链接 | `[[新理论?]]` |

### 链接验证

- 写入记忆时自动解析所有 `[[ ]]` 格式链接
- 验证目标实体/事件/公理是否存在
- 不存在且不是 `?` 结尾的，加入 pending_links 待处理列表
- `?` 结尾的标记为待确认，加入 pending_links

**功能实际文件位置**: `caelix-memory/src/link.rs`

---

## 反向索引系统

### 索引结构

```
ReverseIndex {
    entries: HashMap<entity_name, Vec<IndexEntry>>
}

IndexEntry {
    file: String           // 相对路径，如 Raw/2026-07-08.md
    layer: Layer           // Raw | Wiki | Axiom
    mtime: i64             // 修改时间戳
    snippets: Vec<Snippet> // 匹配的片段列表
}

Snippet {
    heading: String   // 片段所在标题
    hash: String      // 内容哈希（用于去重）
    preview: String   // 预览内容（前100字符）
}
```

### 加权检索

检索结果按以下优先级排序：
1. **层级权重**：Axiom (1.0) > Wiki (0.7) > Raw (0.3)
2. **修改时间**：同层级内按时间倒序
3. **匹配位置**：实体名匹配优先于内容匹配

### 索引构建

- 启动时自动重建（可配置 `auto_rebuild_index`）
- 写入时增量更新对应文件的索引
- 支持手动重建：`caelix memory rebuild-index`

**功能实际文件位置**: `caelix-memory/src/index.rs`

---

## 晋升引擎

### 晋升触发条件

| 晋升方向 | 自动触发条件 |
|---|---|
| Raw → Wiki | 同一实体在 Raw 层被提及 ≥ `raw_mentions_per_entity` 次（默认 3） |
| Raw → Wiki | 当日 Raw 段落数 ≥ `raw_paragraphs_per_day`（默认 10），触发批量整理 |
| Wiki → Axiom | 置信度 ≥ `wiki_confidence_threshold`（默认 0.8） |
| Wiki → Axiom | 且 derived_from 数量 ≥ `wiki_derived_from_min`（默认 3） |

### 晋升路径

```
Raw ──提及次数阈值──→ Wiki 实体
  │
  └──日段落数阈值──→ 批量整理（多实体提取）

Wiki 实体 ──置信度+溯源──→ Axiom 候选
                              │
                              ├─ 置信度 ≥ auto 阈值 → 自动晋升
                              └─ 置信度 ≥ candidate 阈值 → 人工审批
```

### 晋升策略

- **merge**: 合并多个相关记忆
- **refine**: 精炼内容，去除噪声
- **rewrite**: 完全重写，结构化输出

### LLM 预算控制

- 每日 LLM 调用预算（默认 100 次）
- 晋升操作消耗预算
- 预算耗尽后任务进入 deferred 队列
- 次日自动重置

**功能实际文件位置**:
- 晋升引擎：`caelix-memory/src/promote.rs`
- 预算管理：`caelix-memory/src/budget.rs`
- 后台 Worker：`caelix-memory/src/promote_worker.rs`

---

## 冲突与候选管理

### 冲突类型

| 类型 | 触发场景 | 处理方式 |
|---|---|---|
| 实体属性冲突 | 同一实体不同来源属性值不一致 | 标记 + 人工裁决 |
| Axiom 冲突 | 两个公理内容相互矛盾 | 标记 + 人工裁决 |
| 待确认链接 | `[[名称?]]` 或目标不存在 | 标记 + 人工确认 |

### Axiom 候选

- Wiki 实体满足晋升条件但置信度不足时，加入候选列表
- 候选状态：Pending → Approved / Rejected
- 人工审批后决定是否晋升为 Axiom

**功能实际文件位置**: `caelix-memory/src/conflict.rs`

---

## 别名系统

- 每个 Wiki 实体可以有多个别名
- 检索时自动解析别名到规范名
- 重命名实体时自动更新所有引用

**功能实际文件位置**: `caelix-memory/src/alias.rs`

---

## CLI 命令

### 命令清单

| 命令 | 功能 | 示例 |
|---|---|---|
| `caelix memory recall <query>` | 检索记忆 | `caelix memory recall "张三" -k 10` |
| `caelix memory write <content>` | 写入 Raw 层 | `caelix memory write "会议内容" --source meeting` |
| `caelix memory promote --raw <file>` | Raw→Wiki 晋升 | `caelix memory promote --raw 2026-07-08.md` |
| `caelix memory promote --wiki <entity>` | Wiki→Axiom 晋升 | `caelix memory promote --wiki 张三` |
| `caelix memory flags [--all]` | 列出冲突和候选 | `caelix memory flags --all` |
| `caelix memory rebuild-index` | 重建反向索引 | `caelix memory rebuild-index` |
| `caelix memory stats` | 显示统计信息 | `caelix memory stats` |
| `caelix memory axioms` | 查看 Axiom 列表 | `caelix memory axioms --include-deprecated` |
| `caelix memory budget` | 查看 LLM 预算 | `caelix memory budget` |

**功能实际文件位置**: `caelix-bin/src/main.rs`

---

## 数据存储结构

```
~/.caelix/memory_vault/
├── Raw/
│   ├── 2026-07-07.md
│   ├── 2026-07-08.md
│   └── ...
├── Wiki/
│   ├── Entities/
│   │   ├── 张三.md
│   │   ├── 项目A.md
│   │   └── ...
│   └── Events/
│       ├── 项目启动会.md
│       └── ...
├── Axioms/
│   ├── rules/
│   ├── principles/
│   └── methodology/
├── Meta/
│   ├── index.json        # 反向索引
│   ├── aliases.json      # 别名映射
│   ├── conflicts.json    # 冲突和候选
│   ├── budget.json       # LLM 预算
│   └── promotion_log.md  # 晋升日志
```

---

## 线程安全设计

- 使用 `Arc<RwLock>` 包装管理器（alias/index/conflict/budget）
- `tokio::sync::RwLock` 确保 Send 兼容性，支持跨 await 使用
- 写入操作获取写锁，读取操作获取读锁
- 注意避免死锁：按固定顺序获取锁，减少锁持有时间

**核心类型**:
```rust
pub struct MemoryVault {
    raw: RawLayer,
    wiki_entity: WikiEntityLayer,
    wiki_event: WikiEventLayer,
    axiom: AxiomLayer,
    alias: Arc<RwLock<AliasManager>>,
    index: Arc<RwLock<ReverseIndexManager>>,
    conflict: Arc<RwLock<ConflictManager>>,
    budget: Arc<RwLock<LlmBudgetManager>>,
}
```

**功能实际文件位置**: `caelix-memory/src/vault.rs`

---

## API 使用示例

### 1. 初始化 MemoryVault

```rust
use caelix_memory::{MemoryVault, schema::MemoryVaultConfig};

let config = MemoryVaultConfig::default();
let vault = MemoryVault::new(config);
vault.init().await?;
```

### 2. 写入 Raw 记忆

```rust
use caelix_memory::schema::RawSource;
use chrono::Utc;

let today = Utc::now().date_naive();
vault.write_raw(
    today,
    RawSource::Chat,
    vec!["项目".to_string()],
    "10:30",
    "和 [[张三]] 讨论了项目进展，决定采用新的方案。"
).await?;
```

### 3. 检索记忆

```rust
let results = vault.recall("张三", 5).await?;
for result in results {
    println!("[{}] {} - {}", result.layer, result.heading, result.preview);
}
```

### 4. 写入 Wiki 实体

```rust
use caelix_memory::schema::WikiEntityCategory;

vault.write_wiki_entity(
    "张三",
    WikiEntityCategory::Person,
    vec!["老张".to_string()],
    vec!["工程师".to_string()],
    0.9,
    vec!["Raw/2026-07-08.md#10:30".to_string()],
    "## 简介\n\n张三是一位资深工程师..."
).await?;
```

### 5. 写入 Axiom

```rust
use caelix_memory::schema::AxiomCategory;

vault.write_axiom(
    "二八定律",
    AxiomCategory::Principle,
    0.95,
    vec!["Wiki/Entities/效率原则.md".to_string()],
    "## 适用场景\n\n80%的结果来自20%的原因..."
).await?;
```

### 6. 获取统计信息

```rust
let stats = vault.stats().await?;
println!("Raw 文件: {}", stats.raw_files);
println!("Wiki 实体: {}", stats.wiki_entities);
println!("Axiom: {} (活跃: {})", stats.axioms, stats.axioms_active);
```

### 7. 重建索引

```rust
vault.rebuild_index().await?;
```

---

## 业务流程图

### 记忆写入流程

```
用户/Agent → write_raw()
                ↓
         写入 Raw 文件
                ↓
         解析双向链接
                ↓
         验证链接有效性 ──无效/待确认──→ 加入 pending_links
                ↓
         更新反向索引
                ↓
         检查晋升触发条件
                ↓
         触发 PromoteWorker ──→ 提交晋升任务
```

### 记忆检索流程

```
用户/Agent → recall(query, top_k)
                ↓
         别名解析（alias → canonical）
                ↓
         反向索引搜索
                ↓
    ┌───────────────────────┐
    │  实体名匹配           │
    │  + 片段内容匹配       │
    └───────────────────────┘
                ↓
         加权排序（层级 + 时间）
                ↓
         返回 top_k 结果
```

### 晋升流程

```
PromoteWorker → 检查触发条件
                    ↓
            提交晋升任务（TaskManager）
                    ↓
            检查 LLM 预算 ──不足──→ 加入 deferred
                    ↓
            执行晋升逻辑
                    ↓
    ┌───────────────┴───────────────┐
    ↓                               ↓
Raw → Wiki                    Wiki → Axiom
    ↓                               ↓
创建 Wiki 实体              检查冲突 ──有冲突──→ 加入冲突列表
    ↓                               ↓
更新索引                      检查置信度
    ↓                           ┌───┴───┐
记录晋升日志                   ↓       ↓
                           ≥ auto    ≥ candidate
                              ↓       ↓
                        直接晋升   加入候选
                              ↓       ↓
                        创建 Axiom  待审批
```

---

## 配置项说明

```rust
pub struct MemoryVaultConfig {
    pub root_dir: String,           // 根目录，默认 ~/.caelix/memory_vault
    pub auto_rebuild_index: bool,   // 启动时自动重建索引，默认 true
    pub notify_on_promote: bool,    // 晋升时发送通知，默认 true
    pub promote: PromoteConfig,     // 晋升配置
}

pub struct PromoteConfig {
    pub raw_mentions_per_entity: u32,    // Raw→Wiki 提及次数阈值，默认 3
    pub raw_paragraphs_per_day: u32,     // 日段落数阈值触发批量整理，默认 10
    pub wiki_confidence_threshold: f64,  // Wiki→Axiom 置信度阈值，默认 0.8
    pub wiki_derived_from_min: u32,      // Wiki 最少溯源数，默认 3
    pub axiom_auto_promote_confidence: f64,   // 自动晋升置信度，默认 0.95
    pub axiom_candidate_confidence_min: f64,  // 候选最低置信度，默认 0.8
    pub daily_llm_budget: u32,           // 每日 LLM 预算，默认 100
}
```

**功能实际文件位置**: `caelix-memory/src/schema.rs`

---

## 注意事项

### 待完善项

1. **配置系统集成**：当前 CLI 直接使用 `MemoryVaultConfig::default()`，未集成到 caelix-config 的 EnvConfig 中
2. **CLI promote 命令**：当前为占位符，未实际调用晋升引擎
3. **工具注册**：Memory 系列工具未注册到 Caelix 工具系统
4. **Hook 注册**：MemoryCompactorHook 未在系统启动时自动注册
5. **Worker 启动**：PromoteWorker 后台任务未在系统启动时自动启动

### 性能考虑

- 索引重建是 O(n) 操作，大量记忆时可能较慢，建议后台异步执行
- 检索基于内存 HashMap，量级在万级以下性能良好
- 百万级以上需考虑引入 tantivy 等专业搜索引擎

### 安全考虑

- 记忆文件包含用户数据，注意文件权限设置
- 晋升日志可能包含敏感内容，注意访问控制
- 待确认链接 `[[名称?]]` 不应被自动解析为实体

### 并发安全

- 避免在持有写锁时执行 I/O 操作，减少锁持有时间
- 索引重建时先收集所有数据，再加锁一次性写入
- 按固定顺序获取多把锁，防止死锁

---

**最后更新**: 2026-07-08  
**维护者**: Caelix 开发团队
