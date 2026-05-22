# 工具系统规范

## 功能概述

工具系统是 Caelix Agent 与环境交互的桥梁，提供文件操作、搜索、读取等基础能力。通过实现 `Tool` trait 可扩展新工具，支持参数校验、异步执行和错误处理。

## 核心能力

### 1. 工具接口定义

**Tool Trait**:
```rust
#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> JsonValue;
    async fn execute(&self, input: JsonValue) -> ToolResult;
    fn clone_box(&self) -> Box<dyn Tool>;
    
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters_schema: self.parameters_schema(),
        }
    }
}
```

**关键方法**:
- `name()`: 工具唯一标识符
- `description()`: 工具描述，用于 LLM 理解工具用途
- `parameters_schema()`: JSON Schema 格式的参数定义
- `execute()`: 异步执行工具逻辑
- `clone_box()`: 支持克隆（用于 Arc 包装）

### 2. 工具执行流程

```
LLM 请求调用工具 → ToolExecutor 解析参数
                              ↓
                       查找工具实例
                              ↓
                       参数校验 (JSON Schema)
                              ↓
                       执行工具 (异步)
                              ↓
                       捕获错误
                              ↓
                  返回 ToolResult (output/error)
                              ↓
                  转换为 Tool Message
                              ↓
                  添加到消息历史继续对话
```

### 3. 参数校验

**JSON Schema 示例**:
```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "文件路径"
    },
    "content": {
      "type": "string",
      "description": "文件内容"
    }
  },
  "required": ["path", "content"]
}
```

**校验规则**:
- 必填字段检查
- 类型检查（string, number, boolean, array, object）
- 字符串长度限制
- 数值范围限制
- 枚举值检查

### 4. 错误处理

**ToolResult 结构**:
```rust
pub struct ToolResult {
    pub output: String,
    pub error: Option<String>,
}
```

**错误处理策略**:
- 成功: `output` 包含结果，`error` 为 None
- 失败: `error` 包含错误信息，`output` 可为空或包含部分结果
- 错误信息应清晰描述失败原因和建议解决方案

## 内置工具

### 1. DiffEditTool (文件差异编辑)

**功能**: 使用 unified diff 格式编辑文件

**参数**:
```json
{
  "path": "文件路径",
  "diff": "unified diff 格式的差异内容"
}
```

**示例**:
```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,5 @@
 fn main() {
-    println!("Hello");
+    println!("Hello, World!");
 }
```

**安全限制**:
- 路径穿越防护
- 文件大小限制
- 备份原文件（可选）

### 2. DirectoryTreeTool (目录树浏览)

**功能**: 以树形结构展示目录内容

**参数**:
```json
{
  "path": "目录路径",
  "depth": 3,
  "include_hidden": false
}
```

**输出格式**:
```
src/
├── main.rs
├── lib.rs
└── utils/
    ├── mod.rs
    └── helper.rs
```

**性能优化**:
- 限制递归深度
- 忽略 `.git`, `node_modules` 等大目录
- 支持分页显示

### 3. SmartSearchTool (智能文件搜索)

**功能**: 基于内容或文件名搜索文件

**参数**:
```json
{
  "query": "搜索关键词",
  "path": "搜索根目录",
  "file_pattern": "*.rs",
  "case_sensitive": false
}
```

**搜索策略**:
- 文件名模糊匹配
- 文件内容全文搜索
- 支持正则表达式
- 搜索结果排序（相关性）

**性能考虑**:
- 限制搜索文件数量
- 跳过大文件（>1MB）
- 异步并行搜索

### 4. ReadFileTool (文件读取)

**功能**: 读取文件内容

**参数**:
```json
{
  "path": "文件路径",
  "start_line": 1,
  "end_line": 100
}
```

**安全限制**:
- 路径白名单检查
- 文件大小限制
- 二进制文件检测
- 敏感文件过滤（如 `.env`, 私钥文件）

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **DiffEditTool** | `caelix-tools/src/file_edit.rs` | 文件差异编辑实现 |
| **DirectoryTreeTool** | `caelix-tools/src/tree.rs` | 目录树浏览实现 |
| **SmartSearchTool** | `caelix-tools/src/file_search.rs` | 智能搜索实现 |
| **ReadFileTool** | `caelix-tools/src/file_read.rs` | 文件读取实现 |
| **ToolExecutor** | `caelix-agent/src/tool_executor.rs` | 工具执行器 |

### 工具注册

**在 caelix-config 中注册**:
```rust
let mut tools = Vec::new();
tools.push(Arc::new(DiffEditTool::new()) as Arc<dyn Tool>);
tools.push(Arc::new(DirectoryTreeTool::new()) as Arc<dyn Tool>);
// ...
tool_manager.register_tools(tools);
```

**在 Agent 配置中引用**:
```yaml
tools:
  - diff_edit
  - directory_tree
  - global_file_search
  - read_file
```

### 工具执行器

**ToolExecutor 职责**:
1. 解析 LLM 输出的工具调用
2. 查找对应的工具实例
3. 校验参数是否符合 schema
4. 异步执行工具
5. 捕获并格式化错误
6. 返回 ToolResult

**执行流程**:
```rust
pub async fn execute_tool_call(
    &self,
    tool_call: &ToolCall,
    tools: &[Arc<dyn Tool>],
) -> Result<ToolResult, AgentError> {
    // 1. 查找工具
    let tool = tools.iter()
        .find(|t| t.name() == tool_call.name)
        .ok_or_else(|| AgentError::ToolNotFound(tool_call.name.clone()))?;
    
    // 2. 参数校验
    validate_parameters(&tool_call.arguments, &tool.parameters_schema())?;
    
    // 3. 执行工具（带超时）
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        tool.execute(tool_call.arguments.clone())
    ).await??;
    
    Ok(result)
}
```

## 安全规范

### 1. 路径安全

**防护措施**:
```rust
fn sanitize_path(path: &str, allowed_root: &Path) -> Result<PathBuf, AgentError> {
    let full_path = allowed_root.join(path);
    let canonical = full_path.canonicalize()?;
    
    // 防止目录穿越
    if !canonical.starts_with(allowed_root) {
        return Err(AgentError::PathTraversalDetected);
    }
    
    Ok(canonical)
}
```

**检查项**:
- 禁止 `..` 路径穿越
- 限制在允许的工作目录内
- 拒绝绝对路径（除非在白名单中）
- 符号链接解析和验证

### 2. 资源限制

**限制项**:
- 文件大小: 最大 10MB
- 读取行数: 最大 1000 行
- 搜索结果: 最大 100 条
- 执行超时: 30 秒
- 并发工具数: 最大 5 个

**实现**:
```rust
if file.metadata()?.len() > MAX_FILE_SIZE {
    return Err(AgentError::FileTooLarge);
}

tokio::time::timeout(Duration::from_secs(30), tool.execute(input)).await?
```

### 3. 敏感信息保护

**过滤规则**:
- 禁止读取 `.env` 文件
- 禁止读取私钥文件（`*.pem`, `*.key`）
- 禁止读取密码文件（`/etc/shadow`）
- 日志中脱敏文件内容

## 扩展指南

### 添加新工具

**步骤**:

1. **创建工具文件**
```rust
// caelix-tools/src/my_tool.rs
use caelix_api::tool::{Tool, ToolResult};
use serde_json::Value as JsonValue;

#[derive(Debug)]
pub struct MyTool;

impl MyTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }
    
    fn description(&self) -> &str {
        "我的工具描述"
    }
    
    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "param1": {
                    "type": "string",
                    "description": "参数1说明"
                }
            },
            "required": ["param1"]
        })
    }
    
    async fn execute(&self, input: JsonValue) -> ToolResult {
        // 实现工具逻辑
        let param1 = input["param1"].as_str().unwrap();
        
        match self.do_something(param1).await {
            Ok(result) => ToolResult {
                output: result,
                error: None,
            },
            Err(e) => ToolResult {
                output: String::new(),
                error: Some(e.to_string()),
            },
        }
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(self.clone())
    }
}
```

2. **导出工具**
```rust
// caelix-tools/src/lib.rs
pub mod my_tool;
pub use my_tool::MyTool;
```

3. **注册工具**
```rust
// caelix-config/src/tools_loader.rs
tools.push(Arc::new(MyTool::new()) as Arc<dyn Tool>);
```

4. **在 Agent 配置中使用**
```yaml
tools:
  - my_tool
```

### 工具最佳实践

1. **清晰的描述**: description 应详细说明工具用途和使用场景
2. **严格的参数校验**: 使用 JSON Schema 定义清晰的参数约束
3. **详细的错误信息**: 错误信息应指导用户如何修正
4. **合理的超时设置**: 避免长时间阻塞
5. **资源清理**: 确保临时文件和资源被正确清理
6. **幂等性**: 工具执行应是幂等的，可安全重试

## 测试策略

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_diff_edit_tool() {
        let tool = DiffEditTool::new();
        let input = serde_json::json!({
            "path": "test.txt",
            "diff": "--- a/test.txt\n+++ b/test.txt\n@@ -1 +1 @@\n-old\n+new"
        });
        
        let result = tool.execute(input).await;
        assert!(result.error.is_none());
    }
}
```

### 集成测试

- 真实文件系统操作测试
- 多工具组合执行测试
- 错误场景覆盖测试

### Mock 策略

```rust
struct MockTool;

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str { "mock_tool" }
    fn description(&self) -> &str { "Mock tool for testing" }
    fn parameters_schema(&self) -> JsonValue { serde_json::json!({}) }
    
    async fn execute(&self, _input: JsonValue) -> ToolResult {
        ToolResult {
            output: "mock result".to_string(),
            error: None,
        }
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(Self)
    }
}
```

## 性能优化

### 1. 并发执行
```rust
// 并发执行多个独立工具
let results = futures::future::join_all(tool_calls.iter().map(|call| {
    execute_tool_call(call, tools)
})).await;
```

### 2. 缓存策略
- 缓存目录树结构（短期）
- 缓存搜索结果（基于查询哈希）
- 避免重复读取相同文件

### 3. 流式处理
- 大文件分块读取
- 搜索结果流式返回
- 避免一次性加载大量数据到内存

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
