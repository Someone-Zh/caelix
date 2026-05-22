# 配置管理系统规范

## 功能概述

配置管理系统是 Caelix 的资源管理中心，负责从文件系统动态加载和管理 Agent、Provider、Tool、Skill、Command 等各类资源配置。采用 Manager 模式统一管理各类资源，支持热重载和嵌入资源打包。

## 核心能力

### 1. 配置加载架构

**加载器层次**:
```
配置文件 (YAML/Markdown)
      ↓
Loader (解析和验证)
      ↓
Manager (管理和缓存)
      ↓
CaelixContext (统一访问)
```

**支持的配置类型**:
- **Agent**: `.agent` 文件（YAML frontmatter + Markdown）
- **Provider**: `.yaml` 文件（LLM 提供商配置）
- **Tool**: 代码中注册（无独立配置文件）
- **Skill**: `.skill` 文件（Markdown 格式）
- **Command**: `.cmd` 文件（自定义命令）

### 2. Agent 配置

**文件格式** (`planner_agent.agent`):
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
...
```

**解析流程**:
```rust
pub struct AgentsLoader;

impl AgentsLoader {
    pub async fn load_agents(&self, config_dir: &Path) -> Result<Vec<AgentSpec>, ConfigError> {
        let mut agents = Vec::new();
        
        for entry in walkdir::WalkDir::new(config_dir) {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new("agent")) {
                let agent = self.parse_agent_file(path).await?;
                agents.push(agent);
            }
        }
        
        Ok(agents)
    }
    
    fn parse_agent_file(&self, path: &Path) -> Result<AgentSpec, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        
        // 解析 YAML frontmatter
        let (frontmatter, markdown) = split_frontmatter(&content)?;
        let config: AgentConfig = serde_yaml::from_str(&frontmatter)?;
        
        // 构建 AgentSpec
        let tools = self.resolve_tools(&config.tools)?;
        let agent = AgentSpec::with_group(
            config.name,
            markdown.to_string(),
            tools,
            config.group,
        );
        
        Ok(agent)
    }
}
```

### 3. Provider 配置

**文件格式** (`openai_provider.yaml`):
```yaml
name: openai
llm_type: OpenAI
api_key: ${OPENAI_API_KEY}  # 支持环境变量替换
base_url: https://api.openai.com/v1
max_tokens: 4096
temperature: 0.7
models:
  gpt-4: gpt-4
  gpt-3.5-turbo: gpt-3.5-turbo
options:
  timeout: 30
```

**环境变量替换**:
```rust
fn substitute_env_vars(config_str: &str) -> String {
    let re = Regex::new(r"\$\{(\w+)\}").unwrap();
    re.replace_all(config_str, |caps: &regex::Captures| {
        std::env::var(&caps[1]).unwrap_or_default()
    }).to_string()
}
```

**加载流程**:
```rust
pub struct ProviderLoader;

impl ProviderLoader {
    pub async fn load_providers(&self, config_dir: &Path) -> Result<Vec<Arc<dyn LlmProvider>>, ConfigError> {
        let mut providers = Vec::new();
        
        for entry in walkdir::WalkDir::new(config_dir) {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new("yaml")) {
                let provider = self.parse_provider_file(path).await?;
                providers.push(provider);
            }
        }
        
        Ok(providers)
    }
    
    fn parse_provider_file(&self, path: &Path) -> Result<Arc<dyn LlmProvider>, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let content = substitute_env_vars(&content);
        let config: ProviderConfig = serde_yaml::from_str(&content)?;
        
        // 根据 llm_type 创建对应的 Provider
        let provider: Arc<dyn LlmProvider> = match config.llm_type {
            LlmType::OpenAI => Arc::new(OpenAiProvider::new(config)),
        };
        
        Ok(provider)
    }
}
```

### 4. Skill 配置

**文件格式** (`git_commands.skill`):
```markdown
---
name: git_commands
description: Git 常用命令参考
applicable_agents:
  - code_executor_agent
  - planner_agent
---

# Git Commands

## 基本操作
- `git status`: 查看状态
- `git add <file>`: 添加文件
- `git commit -m "message"`: 提交
...
```

**加载流程**:
```rust
pub struct SkillsLoader;

impl SkillsLoader {
    pub async fn load_skills(&self, config_dir: &Path) -> Result<Vec<Skill>, ConfigError> {
        let mut skills = Vec::new();
        
        for entry in walkdir::WalkDir::new(config_dir) {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new("skill")) {
                let skill = self.parse_skill_file(path).await?;
                skills.push(skill);
            }
        }
        
        Ok(skills)
    }
}
```

### 5. Manager 模式

**AgentManager**:
```rust
pub struct AgentManager {
    agents: DashMap<String, Arc<AgentSpec>>,
}

impl AgentManager {
    pub fn register(&self, agent: AgentSpec) {
        let name = agent.name.clone();
        self.agents.insert(name, Arc::new(agent));
    }
    
    pub fn get_agent(&self, name: &str) -> Result<Arc<AgentSpec>, ApiError> {
        self.agents.get(name)
            .map(|a| a.value().clone())
            .ok_or_else(|| ApiError::AgentNotFound(name.to_string()))
    }
    
    pub fn list_agents(&self) -> Vec<String> {
        self.agents.iter().map(|e| e.key().clone()).collect()
    }
    
    pub fn reload(&self, config_dir: &Path) -> Result<(), ConfigError> {
        let loader = AgentsLoader;
        let agents = loader.load_agents(config_dir).await?;
        
        // 清空并重新加载
        self.agents.clear();
        for agent in agents {
            self.register(agent);
        }
        
        Ok(())
    }
}
```

**其他 Manager**:
- **ToolManager**: 管理工具注册和查找
- **ProviderManager**: 管理 LLM Provider
- **SkillManager**: 管理技能加载和匹配
- **CommandManager**: 管理自定义命令

### 6. CaelixContext（全局上下文）

**职责**: 聚合所有 Manager，提供统一访问入口

**结构**:
```rust
pub struct CaelixContext {
    pub agent_manager: Arc<AgentManager>,
    pub tool_manager: Arc<ToolManager>,
    pub provider_manager: Arc<ProviderManager>,
    pub skill_manager: Arc<SkillManager>,
    pub command_manager: Arc<CommandManager>,
    pub session_manager: Arc<SessionManager>,
    pub task_manager: Arc<TaskManager>,
    pub hook_registry: Arc<HookRegistry>,
}

impl CaelixContext {
    pub async fn init(&mut self) -> Result<(), ConfigError> {
        let caelix_home = get_caelix_home();
        
        // 加载所有配置
        self.load_providers(&caelix_home.join("providers")).await?;
        self.load_agents(&caelix_home.join("agents")).await?;
        self.load_skills(&caelix_home.join("skills")).await?;
        self.load_commands(&caelix_home.join("commands")).await?;
        
        // 初始化运行时组件
        self.init_message_bus()?;
        self.init_task_manager()?;
        self.init_hook_registry()?;
        
        Ok(())
    }
}
```

## 技术实现

### 核心组件

| 组件 | 位置 | 职责 |
|------|------|------|
| **AgentsLoader** | `caelix-config/src/agents_loader.rs` | Agent 配置加载器 |
| **ProviderLoader** | `caelix-config/src/provider_loader.rs` | Provider 配置加载器 |
| **SkillsLoader** | `caelix-config/src/skills_loader.rs` | Skill 配置加载器 |
| **CommandsLoader** | `caelix-config/src/commands_loader.rs` | Command 配置加载器 |
| **ToolsLoader** | `caelix-config/src/tools_loader.rs` | Tool 注册器 |
| **AgentManager** | `caelix-config/src/managers/agent.rs` | Agent 管理器 |
| **ToolManager** | `caelix-config/src/managers/tool.rs` | Tool 管理器 |
| **ProviderManager** | `caelix-config/src/managers/provider.rs` | Provider 管理器 |
| **SkillManager** | `caelix-config/src/managers/skill.rs` | Skill 管理器 |
| **CommandManager** | `caelix-config/src/managers/command.rs` | Command 管理器 |
| **CaelixContext** | `caelix-service/src/context.rs` | 全局上下文 |

### 嵌入资源

**使用 rust-embed 打包默认配置**:
```rust
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "conf/"]
struct EmbeddedAssets;

impl EmbeddedAssets {
    pub fn get_default_agents() -> Vec<AgentSpec> {
        let mut agents = Vec::new();
        for file in Self::iter() {
            if file.ends_with(".agent") {
                let content = Self::get(&file).unwrap();
                let agent = parse_agent_from_bytes(content.data.as_ref());
                agents.push(agent);
            }
        }
        agents
    }
}
```

**加载优先级**:
1. 用户配置目录（`$CAELIX_HOME`）
2. 嵌入的默认配置
3. 硬编码的兜底配置

### 配置热重载

**监听文件变化**:
```rust
use notify::{Watcher, RecommendedWatcher, RecursiveMode};

pub struct ConfigWatcher {
    watcher: RecommendedWatcher,
    reload_callback: Box<dyn Fn() + Send + Sync>,
}

impl ConfigWatcher {
    pub fn new<F>(config_dir: &Path, callback: F) -> Result<Self, NotifyError>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    if event.kind.is_modify() {
                        callback();
                    }
                }
            },
            notify::Config::default(),
        )?;
        
        watcher.watch(config_dir, RecursiveMode::Recursive)?;
        
        Ok(Self {
            watcher,
            reload_callback: Box::new(callback),
        })
    }
}
```

**使用示例**:
```rust
let watcher = ConfigWatcher::new(
    &caelix_home.join("agents"),
    || {
        info!("Agent configuration changed, reloading...");
        context.agent_manager.reload(&caelix_home.join("agents")).ok();
    },
)?;
```

## 配置验证

### Agent 配置验证

```rust
fn validate_agent_config(config: &AgentConfig) -> Result<(), ConfigError> {
    // 名称不能为空
    if config.name.is_empty() {
        return Err(ConfigError::InvalidAgentName);
    }
    
    // 工具必须存在
    for tool_name in &config.tools {
        if !tool_registry.exists(tool_name) {
            return Err(ConfigError::ToolNotFound(tool_name.clone()));
        }
    }
    
    // Group 可选，但如果提供则不能为空
    if let Some(group) = &config.group {
        if group.is_empty() {
            return Err(ConfigError::InvalidGroupName);
        }
    }
    
    Ok(())
}
```

### Provider 配置验证

```rust
fn validate_provider_config(config: &ProviderConfig) -> Result<(), ConfigError> {
    // API Key 必填
    if config.api_key.is_empty() {
        return Err(ConfigError::MissingApiKey(config.name.clone()));
    }
    
    // 至少有一个模型
    if config.models.is_empty() {
        return Err(ConfigError::NoModelsDefined(config.name.clone()));
    }
    
    // Base URL 格式校验
    if let Some(url) = &config.base_url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ConfigError::InvalidBaseUrl(url.clone()));
        }
    }
    
    Ok(())
}
```

## 安全规范

### 1. 敏感信息保护

**环境变量注入**:
```yaml
# 不要硬编码 API Key
api_key: ${OPENAI_API_KEY}

# 支持默认值
api_key: ${OPENAI_API_KEY:-sk-default-key}
```

**配置文件权限**:
```bash
chmod 600 ~/.caelix/providers/*.yaml
```

### 2. 路径安全

**限制配置目录**:
```rust
fn validate_config_path(path: &Path, allowed_root: &Path) -> Result<(), ConfigError> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(allowed_root) {
        return Err(ConfigError::PathTraversalDetected);
    }
    Ok(())
}
```

### 3. 配置注入防护

**防止 YAML 注入**:
```rust
// 使用安全的 YAML 解析器，禁用标签解析
let config: AgentConfig = serde_yaml::from_str(&content)?;
// serde_yaml 默认禁用自定义标签，防止任意代码执行
```

## 性能优化

### 1. 配置缓存

**DashMap 并发缓存**:
```rust
pub struct AgentManager {
    agents: DashMap<String, Arc<AgentSpec>>,
}

// 读取无需锁，写入时使用细粒度锁
let agent = manager.agents.get("planner_agent"); // 无锁读取
```

### 2. 懒加载

**按需加载配置**:
```rust
pub fn get_agent(&self, name: &str) -> Result<Arc<AgentSpec>, ApiError> {
    // 先查缓存
    if let Some(agent) = self.agents.get(name) {
        return Ok(agent.value().clone());
    }
    
    // 缓存未命中，从磁盘加载
    let agent = self.load_agent_from_disk(name)?;
    self.agents.insert(name.to_string(), Arc::new(agent.clone()));
    
    Ok(agent)
}
```

### 3. 批量加载

**并行加载多个配置**:
```rust
let agents = futures::future::join_all(
    config_files.iter().map(|path| {
        async move { parse_agent_file(path).await }
    })
).await;
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 处理方式 |
|---------|------|---------|
| `ConfigFileNotFound` | 配置文件不存在 | 使用默认配置 |
| `InvalidConfigFormat` | 配置格式错误 | 返回详细错误信息 |
| `MissingRequiredField` | 缺少必填字段 | 拒绝加载 |
| `ToolNotFound` | 引用的工具不存在 | 拒绝加载 Agent |
| `EnvVarNotSet` | 环境变量未设置 | 使用默认值或报错 |

### 错误恢复

```rust
match self.load_agents(config_dir).await {
    Ok(agents) => {
        info!("Loaded {} agents", agents.len());
        self.register_agents(agents);
    },
    Err(e) => {
        error!("Failed to load agents: {:?}", e);
        // 尝试加载嵌入的默认配置
        warn!("Loading embedded default agents...");
        let default_agents = EmbeddedAssets::get_default_agents();
        self.register_agents(default_agents);
    }
}
```

## 扩展指南

### 添加新配置类型

**步骤**:

1. **定义配置结构**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub settings: HashMap<String, String>,
}
```

2. **创建 Loader**
```rust
pub struct PluginsLoader;

impl PluginsLoader {
    pub async fn load_plugins(&self, config_dir: &Path) -> Result<Vec<PluginConfig>, ConfigError> {
        // 实现加载逻辑
    }
}
```

3. **创建 Manager**
```rust
pub struct PluginManager {
    plugins: DashMap<String, PluginConfig>,
}

impl PluginManager {
    pub fn register(&self, plugin: PluginConfig) { /* ... */ }
    pub fn get_plugin(&self, name: &str) -> Option<PluginConfig> { /* ... */ }
}
```

4. **集成到 CaelixContext**
```rust
pub struct CaelixContext {
    // ...
    pub plugin_manager: Arc<PluginManager>,
}

impl CaelixContext {
    pub async fn init(&mut self) -> Result<(), ConfigError> {
        // ...
        self.load_plugins(&caelix_home.join("plugins")).await?;
        Ok(())
    }
}
```

## 测试策略

### 单元测试

```rust
#[test]
fn test_parse_agent_config() {
    let content = r#"
---
name: test_agent
tools:
  - diff_edit
group: Test
---

Test prompt
"#;
    
    let (frontmatter, markdown) = split_frontmatter(content).unwrap();
    let config: AgentConfig = serde_yaml::from_str(&frontmatter).unwrap();
    
    assert_eq!(config.name, "test_agent");
    assert_eq!(config.tools.len(), 1);
    assert_eq!(markdown.trim(), "Test prompt");
}
```

### 集成测试

- 完整配置加载流程测试
- 热重载功能测试
- 环境变量替换测试
- 嵌入资源加载测试

---

**最后更新**: 2026-05-22  
**维护者**: Caelix 开发团队
