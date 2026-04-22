# Caelix API 使用指南

## 概述

Caelix 现在支持多后端架构，通过 Cargo features 可以编译时选择不同的后端实现。

## 可用的 Features

- `http-server`: 启用 HTTP Server 后端（基于 Axum）
- `tui`: 启用 TUI 后端（基于 Ratatui）
- `all`: 启用所有后端

## 编译选项

### 1. 仅编译 HTTP Server
```bash
cargo build --features http-server
```

### 2. 仅编译 TUI
```bash
cargo build --features tui
```

### 3. 编译所有后端
```bash
cargo build --features all
```

### 4. 默认编译（无后端）
```bash
cargo build
```

## 使用方法

### 演示模式（默认）
```bash
./target/debug/caelix
```

显示默认配置、创建会话并列出可用的 agents。

### HTTP Server 模式
```bash
# 使用默认端口 3000
./target/debug/caelix http

# 指定端口
./target/debug/caelix http 8080
```

启动后可以通过以下 API 端点访问：

#### API 端点

- `GET /api/providers/default` - 获取默认提供者
- `GET /api/models/default` - 获取默认模型
- `POST /api/sessions` - 创建新会话
- `PUT /api/sessions/{session_id}/provider` - 设置会话提供者
- `PUT /api/sessions/{session_id}/model` - 设置会话模型
- `GET /api/agents` - 获取所有 agent 列表
- `POST /api/chat/stream` - 流式聊天（SSE）

#### 示例请求

创建会话：
```bash
curl -X POST http://localhost:3000/api/sessions
```

流式聊天：
```bash
curl -X POST http://localhost:3000/api/chat/stream \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "your-session-id",
    "message": "你好",
    "provider": null,
    "model": null,
    "agent": null
  }'
```

### TUI 模式
```bash
./target/debug/caelix tui
```

启动终端用户界面，支持：
- 输入消息（Enter 发送）
- 查看输出
- 实时流式响应
- Esc 或 Ctrl+C 退出

## 架构说明

### API 层 (src/api/)

- `trait.rs`: 定义 `CaelixApi` trait，提供统一接口
- `types.rs`: 定义请求/响应结构和错误类型
- `session_manager.rs`: 会话管理器，使用 DashMap 实现线程安全存储
- `core.rs`: API 核心实现 `CaelixApiImpl`
- `mod.rs`: 模块入口和导出

### 后端层 (src/backends/)

#### HTTP Backend (src/backends/http/)
- 基于 Axum 框架
- RESTful API 设计
- SSE (Server-Sent Events) 流式输出
- CORS 支持

#### TUI Backend (src/backends/tui/)
- 基于 Ratatui + Crossterm
- 响应式界面
- 异步事件处理
- 实时流式更新

## 核心功能

### 会话管理
- 每个会话有独立的提供者、模型、agent 配置
- 使用 UUID 生成唯一会话 ID
- 线程安全的内存存储

### 流式聊天
- 支持 SSE 流式输出（HTTP）
- 异步通道传输（TUI）
- 实时显示 Agent 输出片段

### 配置管理
- 会话级别的配置覆盖
- 默认配置 fallback
- 动态切换提供者和模型

## 开发提示

### 添加新的后端
1. 在 `src/backends/` 创建新模块
2. 在 `Cargo.toml` 添加对应的 feature
3. 在 `src/backends/mod.rs` 添加条件编译
4. 在 `src/main.rs` 添加启动逻辑

### 扩展 API
1. 在 `CaelixApi` trait 中添加新方法
2. 在 `CaelixApiImpl` 中实现
3. 在各后端中添加相应的处理器

## 故障排除

### 编译错误
确保已安装所有依赖：
```bash
cargo update
```

### 运行时错误
检查配置文件是否正确初始化：
```bash
# 确保 providers.json 等配置文件存在
ls ~/.caelix/
```

### TUI 显示问题
确保终端支持 UTF-8 和真彩色。
