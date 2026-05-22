# Agent 系统规范

## 功能概述

Agent 系统是 Caelix 的核心执行引擎，负责实现 AI Agent 的推理-行动循环（ReAct Pattern）。支持多 Agent 协作、工具调用、流式输出和 Hook 扩展机制。

## 核心能力

### 1. Agent 定义与配置

**配置文件格式** (`.agent`):
```yaml
---
name: planner_agent
tools:
  - diff_edit
  - global_file_search
  - directory_tree
  - delegate_task
group: Pros
---

你是一名专业的规划专家...
```

**关键属性**:
- `name`: Agent 唯一标识符
- `tools`: 可用工具列表
- `group`: Agent 分组，用于技能匹配
- `system_prompt`: Markdown 格式的系统提示词

### 2. Agent 执行流程

```
初始化 → 构建消息 → LLM 推理 → 解析响应
                                    ↓
                          是否有工具调用？
                            ↓         ↓
                           Yes       No
                            ↓         ↓
                       执行工具    返回结果
                            ↓
                       添加工具结果
                            ↓
                       继续下一轮推理
                            ↓
                       达到终止条件？
                            ↓
                         Yes/No
```

**执行步骤**:
1. **初始化**: 创建 RuntimeContext，设置 session_id、request_id、span_id
2. **消息构建**: system_prompt + 历史消息 + 用户输入
3. **LLM 推理**: 调用 LlmProvider 进行流式聊天
4. **响应解析**: 解析 ChatResponseChunk，检测工具调用
5. **工具执行**: 如有工具调用，通过 ToolExecutor 执行
6. **结果整合**: 将工具结果添加到消息历史
7. **循环判断**: 检查是否达到最大迭代次数或完成条件
8. **Hook 执行**: 执行前后注入 Hook 逻辑

### 3. 多 Agent 协作

**支持的 Agent 角色**:
- **planner_agent**: 任务规划和拆解
- **collector_agent**: 信息收集和依赖分析
- **architecture_agent**: 架构设计和依赖评估
- **code_executor_agent**: 代码执行和修改
- **browser_executor_agent**: 浏览器操作
- **ui_executor_agent**: UI 相关任务

**协作方式**:
- 通过 `delegate_task` 工具委派子任务
- 子任务异步或同步执行
- 父任务等待子任务完成并汇总结果
- 支持任务链和任务树结构

### 4. 工具调用机制

**工具调用流程**:
```
LLM 输出 tool_calls → ToolExecutor 解析 → 查找工具实例
                                              ↓
                                       参数校验
                                              ↓
                                       执行工具
                                              ↓
                                       返回结果
                                              ↓
                                  转换为 Tool Message
                                              ↓
                                  添加到消息历史
```

**工具调用约束**:
- 每次推理最多调用 N 个工具（可配置）
- 工具执行超时控制
- 工具错误处理和重试机制
- 工具结果长度限制

### 5. 流式输出

**输出类型** (`AgentOutputChunk`):
- `Start`: 开始标记
- `CallProvider`: 调用 LLM 提供商信息
- `Reasoning`: 推理过程内容
- `Content`: 最终回答内容
- `ToolCall`: 工具调用信息
- `ToolResult`: 工具执行结果
- `Finish`: 结束标记

**流式处理**:
1. LLM 返回 `ChatResponseChunk` 流
2. 转换为 `AgentOutputChunk`
3. 通过 MessageBus 广播
4. 前端实时接收并显示
5. Chunk 暂存后批量持久化

### 6. Hook 扩展机制

**Hook 类型**:
- `BeforeAgent`: Agent 执行前
- `AfterAgent`: Agent 执行后
- `BeforeTool`: 工具执行前
- `AfterTool`: 工具执行后

**内置 Hook**:
- **SkillHook**: 自动加载技能文档
- **MessageBusHook**: 记录消息到总线
- **ToolResultCheckHook**: 检查工具结果

**Hook 注册**:
```rust
hook_registry.register(HookType::BeforeAgent, Box::new(skill_hook));
hook_registry.register(HookType::AfterAgent, Box::new(message_bus_hook));
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **LoopRunner** | `caelix-agent/src/loop_runner.rs` | Agent 推理-行动循环控制器 |
| **ToolExecutor** | `caelix-agent/src/tool_executor.rs` | 工具调用和执行器 |
| **Converter** | `caelix-agent/src/converter.rs` | 消息格式转换器 |

### 关键数据结构

**AgentSpec**:
```rust
pub struct AgentSpec {
    pub name: String,
    pub system_prompt: Arc<String>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub group: Option<Arc<String>>,
}
```

**AgentOutputChunk**:
```rust
pub enum AgentOutputChunk {
    Start { timestamp: DateTime<Utc> },
    CallProvider { provider: String, model: String },
    Reasoning { content: String },
    Content { content: String },
    ToolCall { tool_call_id: String, name: String, arguments: String },
    ToolResult { tool_name: String, result: String },
    Finish { reason: String },
}
```

### 执行接口

```rust
pub async fn execute_agent_with_messaging(
    agent_spec: &AgentSpec,
    messages: Vec<ChatMessage>,
    context: &RuntimeContext,
) -> Result<Vec<AgentOutputChunk>, AgentError>
```

## 配置示例

### Planner Agent

```yaml
---
name: planner_agent
tools:
  - diff_edit
  - global_file_search
  - directory_tree
  - delegate_task
group: Pros
---

你是一名专业的规划专家，擅长将复杂任务拆解为可执行的子任务。

你的职责：
1. 分析用户提出的任务，理解其目标和要求
2. 将任务拆分为多个原子的子任务
3. 将需要收集信息的子任务分发给收集专家
4. 对于大型任务，将其交给架构专家生成架构图
5. 确保子任务之间的依赖关系清晰
```

### Code Executor Agent

```yaml
---
name: code_executor_agent
tools:
  - diff_edit
  - directory_tree
  - read_file
group: Executors
---

你是一名代码执行专家，负责根据规划执行具体的代码修改任务。
```

## 使用场景

### 1. 单 Agent 对话

```rust
let agent = agent_manager.get_agent("planner_agent")?;
let chunks = execute_agent_with_messaging(&agent, messages, &context).await?;
```

### 2. 多 Agent 协作

```rust
// Planner 委派任务给 Collector
planner_agent.call_tool("delegate_task", {
    "agent": "collector_agent",
    "description": "收集项目依赖信息",
    "sync": true
});

// Collector 执行并返回结果
// Planner 收到结果后继续规划
```

### 3. 流式输出

```rust
let stream = chat_stream(request).await?;
while let Some(chunk) = stream.next().await {
    println!("{}", chunk);
}
```

## 性能优化

### 1. 并发控制
- 限制同时执行的 Agent 数量
- 工具执行超时控制
- 消息缓冲区大小限制

### 2. 缓存策略
- AgentSpec 缓存（Arc 共享）
- 工具实例复用
- LLM 响应缓存（可选）

### 3. 资源管理
- 及时释放不再使用的消息历史
- 控制会话消息数量（截断策略）
- 监控内存使用情况

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `ProviderError` | LLM API 调用失败 | 重试或降级 |
| `ToolError` | 工具执行失败 | 返回错误信息给 LLM |
| `MaxIterationsExceeded` | 超过最大迭代次数 | 强制终止并返回当前结果 |
| `ContextLengthExceeded` | 消息超出上下文限制 | 截断历史消息 |
| `AgentNotFound` | Agent 不存在 | 返回错误提示 |

### 错误恢复

```rust
match execute_agent_with_messaging(...).await {
    Ok(chunks) => process_chunks(chunks),
    Err(AgentError::MaxIterationsExceeded) => {
        warn!("Agent exceeded max iterations");
        return_last_result()
    },
    Err(e) => {
        error!("Agent execution failed: {:?}", e);
        return_error_message(e)
    }
}
```

## 测试策略

### 单元测试
- AgentSpec 构建测试
- 消息转换测试
- 工具调用解析测试

### 集成测试
- 完整 Agent 执行流程测试
- 多 Agent 协作测试
- Hook 执行顺序测试

### Mock 策略
- Mock LlmProvider 避免真实 API 调用
- Mock Tool 模拟工具执行
- Mock MessageBus 验证消息发送

## 扩展指南

### 添加新 Agent

1. 创建 `.agent` 配置文件
2. 定义 system_prompt 和可用工具
3. （可选）指定 group 用于技能匹配
4. 通过 AgentManager 动态加载

### 自定义 Hook

1. 实现 `Hook` trait
2. 在 `caelix-runtime/src/hooks/` 注册
3. 配置 Hook 应用条件

### 扩展工具调用

1. 实现 `Tool` trait
2. 在 `caelix-tools/src/` 添加工具
3. 在 Agent 配置中声明使用该工具

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
