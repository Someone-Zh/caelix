# Caelix Security

安全检测模块,提供文件路径和 URL 访问控制功能。

## 功能特性

- 路径白名单/黑名单机制
- URL 模式匹配(支持通配符)
- 防止路径穿越攻击
- 运行时配置管理
- 配置持久化

## 使用示例

```rust
use caelix_security::{SecurityChecker, SecurityConfig, PathSecurityConfig, UrlSecurityConfig};

// 创建配置
let config = SecurityConfig {
    path: PathSecurityConfig {
        include: vec!["/home/user/projects".to_string()],
        exclude: vec!["/home/user/projects/.git".to_string()],
    },
    url: UrlSecurityConfig {
        include: vec!["https://api.example.com/*".to_string()],
        exclude: vec![],
    },
};

// 创建检查器
let checker = SecurityChecker::new(config);

// 检查路径
if checker.is_path_safe("/home/user/projects/myapp").await {
    println!("Path is safe");
}

// 检查 URL
if checker.is_url_safe("https://api.example.com/v1/users").await {
    println!("URL is safe");
}

// 动态添加规则
checker.add_path_include("/tmp".to_string()).await?;
checker.add_url_exclude("http://blocked.com/*".to_string()).await?;
```

## 配置文件

在 `$CAELIX_HOME/config.json` 中配置:

```json
{
  "security": {
    "path": {
      "include": ["/allowed/path"],
      "exclude": ["/forbidden/path"]
    },
    "url": {
      "include": ["https://trusted.com/*"],
      "exclude": ["http://blocked.com"]
    }
  }
}
```

配置加载使用 `caelix_security::loader::load_security_config()` 函数,该函数会:
1. 检查 `$CAELIX_HOME/config.json` 是否存在
2. 如果不存在,创建默认配置(空列表)
3. 读取并解析 JSON 配置

## API 说明

### SecurityChecker

主要的安全检查器类,提供以下方法:

- `is_path_safe(path: &str) -> bool`: 检查路径是否安全
- `is_url_safe(url: &str) -> bool`: 检查 URL 是否安全
- `add_path_include(path: String) -> Result<(), SecurityError>`: 添加允许路径
- `add_path_exclude(path: String) -> Result<(), SecurityError>`: 添加排除路径
- `add_url_include(pattern: String) -> Result<(), SecurityError>`: 添加允许 URL 模式
- `add_url_exclude(pattern: String) -> Result<(), SecurityError>`: 添加排除 URL 模式
- `get_config() -> SecurityConfig`: 获取当前配置
- `reload_config(new_config: SecurityConfig)`: 重新加载配置

### 路径检测规则

1. 如果路径在 exclude 列表中或其子目录,返回 false
2. 如果路径在 include 列表中或其子目录,返回 true
3. 否则返回 false

### URL 检测规则

1. 如果 URL 匹配 exclude 模式,返回 false
2. 如果 URL 匹配 include 模式,返回 true
3. 否则返回 false

支持 glob 通配符模式,如 `https://*.example.com/*` 或 `http://localhost:*`

## 安全特性

### 路径穿越防护

使用 `sanitize_path()` 函数检测和阻止路径穿越攻击:

```rust
use caelix_security::path_checker::sanitize_path;

// 这些会返回错误
sanitize_path("../etc/passwd")?;
sanitize_path("/home/user/../secret")?;

// 这是安全的
sanitize_path("/home/user/./file.txt")?;
```

## 集成到 CaelixContext

SecurityChecker 已自动集成到 CaelixContext 中,在应用启动时从 `config.json` 加载配置:

```rust
let context = CaelixContext::new();
context.init().await?;

// 使用安全检查器
if context.security_checker.is_path_safe("/some/path").await {
    // 执行文件操作
}
```

## 注意事项

1. **默认策略**: 如果配置为空列表,默认拒绝所有访问
2. **并发安全**: 使用 `Arc<RwLock>` 确保多线程安全
3. **配置持久化**: 通过 `caelix_security::loader` 实现配置持久化
4. **性能考虑**: 路径检测优先使用字符串匹配,避免频繁的文件系统调用
5. **依赖关系**: caelix-security 依赖 caelix-config,caelix-config 是底层包
