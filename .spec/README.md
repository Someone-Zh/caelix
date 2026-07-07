# Caelix 项目规范文档

本目录包含 Caelix 项目的完整企业级规范文档，所有开发人员必须严格遵守这些规范。

## 文档结构

```
.spec/
├── README.md                    # 本文件
├── rules.md                     # ⭐ 全局开发规范（每次开发前必读）
├── spec.md                      # ⭐ 项目结构规范（每次开发前必读）
│
├── caelix-agent/                # Agent 系统规范
│   └── spec.md
│
├── caelix-tools/                # 工具系统规范
│   └── spec.md
│
├── caelix-message/              # 消息总线系统规范
│   └── spec.md
│
├── caelix-task/                 # 任务调度系统规范
│   └── spec.md
│
├── caelix-runtime/              # 运行时系统规范
│   └── spec.md
│
├── caelix-config/               # 配置管理系统规范
│   └── spec.md
│
├── caelix-cli/                  # CLI 界面规范
│   └── spec.md
│
├── caelix-http/                 # HTTP API 服务规范
│   └── spec.md
│
├── caelix-tui/                  # TUI 界面规范
│   └── spec.md
│
├── caelix-memory/               # 记忆系统规范
│   └── spec.md
│
└── caelix-llm/                  # LLM Provider 规范
    └── spec.md
```

## 核心文档

### 1. rules.md - 全局开发规范

**重要性**: ⭐⭐⭐⭐⭐ （最高优先级）

**内容**:
- 技术栈说明
- 架构模式
- 命名规则
- 代码风格
- 安全规范
- 测试规范
- 性能规范

**使用场景**: 
- 每次编写代码前必须查阅
- Code Review 的检查依据
- 新成员入职必读

### 2. spec.md - 项目结构规范

**重要性**: ⭐⭐⭐⭐⭐ （最高优先级）

**内容**:
- 项目整体架构
- 模块职责表
- 查找位置表格
- 数据流向图
- 依赖关系
- 功能清单

**使用场景**:
- 理解项目整体结构
- 查找功能实现位置
- 理解模块间依赖
- 设计新功能时参考

## 功能模块规范

每个功能模块的 spec.md 包含：

### 标准章节

1. **功能概述**: 模块的核心职责和定位
2. **核心能力**: 模块提供的主要功能
3. **技术实现**: 关键组件和数据结构
4. **使用示例**: 代码示例和最佳实践
5. **扩展指南**: 如何扩展该模块
6. **错误处理**: 常见错误和处理策略
7. **性能优化**: 性能考虑和优化建议
8. **测试策略**: 单元测试和集成测试指南

### 模块列表

| 模块 | 文档路径 | 核心职责 |
|------|---------|---------|
| **Agent 系统** | `caelix-agent/spec.md` | Agent 执行引擎、推理-行动循环、多 Agent 协作 |
| **工具系统** | `caelix-tools/spec.md` | 文件操作、搜索、读取等基础工具 |
| **消息总线** | `caelix-message/spec.md` | 消息传递、会话管理、持久化存储 |
| **任务调度** | `caelix-task/spec.md` | 任务队列、调度、委派、定时任务 |
| **运行时系统** | `caelix-runtime/spec.md` | RuntimeContext、Hook 系统、ID 生成 |
| **配置管理** | `caelix-config/spec.md` | 配置加载、资源管理、热重载 |
| **CLI 界面** | `caelix-cli/spec.md` | 命令行交互、命令处理、流式输出 |
| **HTTP API** | `caelix-http/spec.md` | RESTful API、SSE、CORS、错误处理 |
| **TUI 界面** | `caelix-tui/spec.md` | 终端图形界面、视图渲染、事件处理 |
| **记忆系统** | `caelix-memory/spec.md` | 三层架构记忆（Raw/Wiki/Axiom）、双向链接、反向索引、晋升引擎 |
| **LLM Provider** | `caelix-llm/spec.md` | LLM API 集成、流式响应、工具调用 |

## 使用指南

### 日常开发流程

1. **开始新功能前**:
   - 阅读 `rules.md` 了解编码规范
   - 阅读 `spec.md` 了解项目结构
   - 查阅相关功能模块的 spec.md

2. **编写代码时**:
   - 遵循 `rules.md` 中的命名规则和代码风格
   - 参考功能模块 spec.md 中的实现示例
   - 确保符合安全规范和性能要求

3. **Code Review 时**:
   - 对照 `rules.md` 检查代码规范
   - 验证是否符合模块职责划分
   - 检查错误处理和测试覆盖

### 查找信息

**快速索引**:
- 想了解命名规范？ → `rules.md` → "命名规则"
- 想找到某个功能的位置？ → `spec.md` → "查找位置表格"
- 想了解如何添加新工具？ → `caelix-tools/spec.md` → "扩展指南"
- 想了解 Hook 系统？ → `caelix-runtime/spec.md` → "Hook 系统"

### 新增功能模块

如果需要为新模块创建规范文档：

1. 在 `.spec/` 下创建模块目录
2. 创建 `spec.md` 文件
3. 按照标准章节模板编写
4. 在 `spec.md` 的"项目下功能"表格中添加链接
5. 在本 README 中更新模块列表

## 维护规范

### 文档更新原则

1. **代码变更时同步更新文档**:
   - API 变更 → 更新相关 spec.md
   - 新增功能 → 创建或更新对应文档
   - 重构代码 → 更新架构描述

2. **保持文档准确性**:
   - 不得虚构不存在的信息
   - 代码示例必须可运行
   - 定期审查文档与代码的一致性

3. **文档版本管理**:
   - 在文档末尾标注"最后更新"日期
   - 重大变更时在 commit message 中说明
   - 保留历史版本的变更记录

### 文档质量标准

- ✅ 使用中文编写
- ✅ 格式清晰，使用 Markdown 标题和代码块
- ✅ 包含足够的代码示例
- ✅ 图表辅助说明（使用 Mermaid 或 ASCII art）
- ✅ 链接到相关文档和代码文件

## 贡献指南

### 提交文档修改

1. 确保修改符合规范格式
2. 检查拼写和语法错误
3. 验证代码示例的正确性
4. 更新"最后更新"日期
5. 在 PR 中说明修改原因

### 反馈问题

如果发现文档问题：
- 提交 Issue 标注 `[Documentation]` 标签
- 说明具体问题和建议改进方案
- 欢迎直接提交 PR 修复

## 附录

### 相关资源

- [Rust 官方文档](https://doc.rust-lang.org/)
- [Tokio 异步编程指南](https://tokio.rs/)
- [Axum Web 框架文档](https://docs.rs/axum/)
- [Ratatui TUI 框架文档](https://ratatui.rs/)

### 联系方式

- 项目仓库: [GitHub](https://github.com/your-org/caelix)
- 问题反馈: [Issues](https://github.com/your-org/caelix/issues)
- 讨论区: [Discussions](https://github.com/your-org/caelix/discussions)

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
