# 技能系统使用指南

## 概述

技能系统为Caelix提供了可扩展的能力增强机制,允许通过技能文件定义专业知识,并在Agent执行时自动注入这些能力。

## 核心特性

### 1. 技能管理
- **Skill模型**: 包含名称、命名空间、描述和内容
- **SkillManager**: 负责技能的注册、查询和管理
- **命名空间支持**: 支持多级目录嵌套,如 `a::b::c::skill_name`

### 2. 技能加载
- **文件格式**: `.skill` 文件,采用 YAML头 + Markdown内容格式
- **递归扫描**: 自动扫描 `~/.caelix/skills/` 目录及其子目录
- **动态加载**: 启动时自动加载所有可用技能

### 3. Agent增强钩子
- **HookRegistry**: 全局钩子注册中心
- **AgentHook trait**: 定义钩子接口,允许在Agent执行前修改AgentSpec
- **SkillHook**: 内置技能钩子,自动为Agent添加技能列表和get_skill_detail工具

### 4. 技能工具
- **get_skill_detail**: 获取指定技能的详细内容
- **参数**: `skill_name` (完整名称,包含命名空间)
- **返回**: 技能的名称、命名空间、描述和完整内容

## 使用指南

### 创建技能文件

技能文件位于 `~/.caelix/skills/` 目录下,使用 `.skill` 扩展名。

**文件格式:**
```yaml
---
name: skill_name
description: 技能的简短描述
---

# 技能内容(Markdown格式)

这里是技能的详细内容,可以包括:
- 使用说明
- 最佳实践
- 示例代码
- 常见问题
```

**示例:**
```yaml
---
name: git
description: Git版本控制操作技能
---

# Git 操作技能

## 基本命令
- `git status` - 查看状态
- `git commit` - 提交更改
...
```

### 命名空间规则

命名空间由目录结构决定:

```
~/.caelix/skills/
├── coding/
│   ├── git.skill          → coding::git
│   └── python/
│       └── pytest.skill   → coding::python::pytest
└── writing/
    └── email.skill        → writing::email
```

### 技能自动注入

当Agent执行时,系统会自动:

1. **应用钩子**: HookRegistry对所有注册的钩子依次调用
2. **添加技能列表**: 在Agent的system_prompt中添加可用技能列表
3. **添加工具**: 自动添加 `get_skill_detail` 工具

**效果示例:**

Agent的system_prompt会被增强为:
```
[原始system_prompt]

## Available Skills

You have access to the following skills:
- coding::git
- writing::email

Use the 'get_skill_detail' tool to view the full content of any skill when needed.
```

### 使用技能

Agent可以通过以下方式利用技能:

1. **查看可用技能**: 从system_prompt中了解有哪些技能可用
2. **获取技能详情**: 调用 `get_skill_detail` 工具
   ```json
   {
     "skill_name": "coding::git"
   }
   ```
3. **应用技能知识**: 根据技能内容执行任务

## API参考

### SkillManager

```rust
// 注册技能
await skill_manager.register(skill)?;

// 获取技能
let skill = skill_manager.get("coding::git").await;

// 列出所有技能
let all_skills = skill_manager.list_all().await;

// 按命名空间列出
let coding_skills = skill_manager.list_by_namespace("coding").await;
```

### HookRegistry

```rust
// 注册钩子
hook_registry.register_hook(Arc::new(MyHook)).await;

// 应用钩子到AgentSpec
hook_registry.apply_hooks(&mut agent_spec).await;

// 获取钩子数量
let count = hook_registry.hook_count().await;
```

### GetSkillDetailTool

```rust
// 工具定义
name: "get_skill_detail"
parameters: {
  "skill_name": "string"  // 必需,技能的完整名称
}

// 返回结果
{
  "output": "# Skill Name\n\n**Namespace:** ...\n...",
  "error": null
}
```

## 扩展示例

### 创建自定义钩子

```rust
use crate::enhancement::hooks::AgentHook;
use crate::base::agent::AgentSpec;

pub struct MyCustomHook;

impl AgentHook for MyCustomHook {
    fn name(&self) -> &str {
        "my_custom_hook"
    }
    
    fn enhance_agent(&self, agent_spec: &mut AgentSpec) {
        // 修改system_prompt
        agent_spec.system_prompt.push_str("\n\nCustom instruction...");
        
        // 添加工具
        agent_spec.tools.push(my_tool);
    }
}

// 注册钩子
context.hook_registry.register_hook(Arc::new(MyCustomHook)).await;
```

## 最佳实践

1. **技能粒度**: 每个技能专注于一个特定领域或任务
2. **清晰描述**: 在YAML头中提供准确的描述,帮助Agent理解何时使用该技能
3. **结构化内容**: 使用Markdown标题、列表和代码块组织内容
4. **命名规范**: 使用有意义的命名空间和技能名称
5. **避免冗余**: 不要在多个技能中重复相同的内容

## 故障排除

### 技能未加载

检查:
1. 技能文件是否在 `~/.caelix/skills/` 目录下
2. 文件格式是否正确(YAML头 + Markdown内容)
3. 文件扩展名是否为 `.skill`
4. 查看启动日志中的 "Loading skill from" 消息

### 技能未生效

检查:
1. skill_hook是否成功注册(查看 "Registering hook: skill_hook" 日志)
2. Agent执行时是否调用了 `apply_hooks`
3. system_prompt中是否包含技能列表

### 命名空间问题

确保:
1. 目录结构正确反映了期望的命名空间
2. 路径分隔符使用 `/` 或 `\`(跨平台兼容)
3. 技能名称不包含 `::` 字符

## 技术架构

```
┌─────────────────────────────────────┐
│      CaelixContext                  │
│  ┌──────────────┐  ┌─────────────┐ │
│  │SkillManager  │  │HookRegistry │ │
│  └──────┬───────┘  └──────┬──────┘ │
└─────────┼──────────────────┼────────┘
          │                  │
          │                  │
┌─────────▼──────────────────▼────────┐
│         SkillHook                   │
│  ┌──────────────────────────────┐   │
│  │ • 添加技能列表到prompt       │   │
│  │ • 添加get_skill_detail工具   │   │
│  └──────────────────────────────┘   │
└─────────────────────────────────────┘
          │
          │ 执行时应用
          ▼
┌─────────────────────────────────────┐
│         AgentSpec (Enhanced)        │
│  • system_prompt + 技能列表         │
│  • tools + get_skill_detail         │
└─────────────────────────────────────┘
```

## 相关文件

- `src/manager/skill.rs` - Skill模型和SkillManager
- `src/config/skills_loader.rs` - 技能加载器
- `src/enhancement/hooks/mod.rs` - AgentHook trait
- `src/enhancement/hooks/skill_hook.rs` - SkillHook实现
- `src/base/tool/get_skill.rs` - GetSkillDetailTool
- `src/enhancement/mod.rs` - HookRegistry
- `src/config/context.rs` - 集成点
