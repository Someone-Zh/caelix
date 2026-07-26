# caelix-tui 重构计划

## 一、现状调研结论

### 当前 caelix-tui 包结构
```
caelix-tui/
├── Cargo.toml
└── src/
    ├── lib.rs          # 模块导出 + pub use runner::run_tui
    ├── runner.rs       # 主循环 + 事件分发 + 终端初始化 (~385 行)
    ├── commands.rs     # 命令处理器 (~293 行)
    ├── events.rs       # 事件处理器 + TuiEvent 枚举 (~71 行)
    ├── state.rs        # App 状态 + 所有数据模型 (~417 行)
    └── views.rs        # Ratatui 渲染逻辑 (~814 行)
```

### 现有问题
1. **无分层架构**：所有逻辑混在一起，UI 直接调用 API、直接读写状态
2. **职责不清**：state.rs 同时承担领域模型、应用状态、业务逻辑
3. **违反红线**：UI 层（runner/views）直接与 caelix-service 耦合，可直接调用后端 API
4. **不可测试**：业务逻辑无法脱离 Ratatui 进行单元测试
5. **缺少功能**：无打字机动画、无 MD 渲染、无精美的启动 Logo

---

## 二、重构目标架构（四层 DDD 风格）

```
caelix-tui/src/
├── lib.rs                  # 模块导出
├── main.rs                 # (可选) 独立运行入口
├── domain/                 # 【领域层】纯模型、枚举、业务常量
│   ├── mod.rs
│   ├── message.rs          # Message, MessageRole, MessageId
│   ├── task.rs             # Task, TaskStatus, TaskProgress
│   ├── notification.rs     # Notification, NotificationLevel
│   ├── session.rs          # Session, SessionId
│   └── constants.rs        # UI 常量（颜色、尺寸、动画速度等）
├── infrastructure/         # 【基础设施层】基于 trait 的外部能力实现
│   ├── mod.rs
│   ├── traits.rs           # 定义所有 trait（ChatService, TaskService, ...）
│   └── mock/               # Mock 实现（当前阶段使用）
│       ├── mod.rs
│       └── mock_services.rs
├── application/            # 【应用服务层】AppService 编排一切
│   ├── mod.rs
│   └── app_service.rs      # AppService: 持有领域状态 + 基础设施依赖
└── ui/                     # 【UI 表现层】纯 Ratatui
    ├── mod.rs
    ├── app.rs              # TuiApp: UI 状态机 + 事件处理（只调 AppService）
    ├── event.rs            # 事件封装（crossterm → UiEvent）
    ├── renderer.rs         # 主渲染调度
    ├── widgets/            # 自定义组件
    │   ├── mod.rs
    │   ├── splash.rs       # 启动 Logo + 光影动画
    │   ├── chat_area.rs    # 左侧 80%: 消息渲染 + 打字机
    │   ├── input_area.rs   # 输入区（ratatui-textarea）
    │   ├── sidebar.rs      # 右侧 20%: 待办/通知/进度
    │   └── markdown.rs     # Markdown 渲染组件
    └── theme.rs            # 主题/配色
```

### 架构红线（强制执行）
1. **UI 层绝不直接使用 `reqwest`**
2. **UI 层绝不直接读写文件系统**（`std::fs`, `std::io::File` 等）
3. **UI 层只依赖 `application::AppService`**，通过其公开方法完成所有业务操作
4. **领域层无任何外部依赖**（只可用 std + serde 等基础库）
5. **基础设施层通过 trait 抽象**，具体实现可替换（Mock → 真实后端）

---

## 三、文件与模块改动清单

### 需要删除（清空内容/删除文件）
| 文件 | 操作 | 原因 |
|------|------|------|
| `src/runner.rs` | 删除 | 所有逻辑迁移到新架构 |
| `src/commands.rs` | 删除 | 业务逻辑进入 AppService |
| `src/events.rs` | 删除 | 重构为 `ui/event.rs` |
| `src/state.rs` | 删除 | 拆分到 domain + application |
| `src/views.rs` | 删除 | 拆分到 ui/widgets/ + ui/renderer.rs |

### 需要新建
| 文件 | 层级 | 职责 |
|------|------|------|
| `src/domain/mod.rs` | Domain | 导出领域模块 |
| `src/domain/message.rs` | Domain | 消息模型（Message, MessageRole） |
| `src/domain/task.rs` | Domain | 任务模型（Task, TaskStatus, TaskProgress） |
| `src/domain/notification.rs` | Domain | 通知模型 |
| `src/domain/session.rs` | Domain | 会话模型 |
| `src/domain/constants.rs` | Domain | 业务常量（动画速度、颜色名等） |
| `src/infrastructure/mod.rs` | Infra | 导出基础设施模块 |
| `src/infrastructure/traits.rs` | Infra | ChatService, TaskService, NotificationService trait 定义 |
| `src/infrastructure/mock/mod.rs` | Infra | Mock 模块导出 |
| `src/infrastructure/mock/mock_services.rs` | Infra | Mock 服务实现（返回假数据） |
| `src/application/mod.rs` | App | 导出应用服务 |
| `src/application/app_service.rs` | App | AppService 核心：所有业务方法（send_message, list_tasks, ...） |
| `src/ui/mod.rs` | UI | 导出 UI 模块 |
| `src/ui/app.rs` | UI | TuiApp: UI 状态机，事件循环，只调用 AppService |
| `src/ui/event.rs` | UI | UiEvent 枚举 + crossterm 转换 |
| `src/ui/renderer.rs` | UI | 主渲染入口，调度各子组件 |
| `src/ui/theme.rs` | UI | 颜色/样式主题定义 |
| `src/ui/widgets/mod.rs` | UI | 组件导出 |
| `src/ui/widgets/splash.rs` | UI | 启动页：Caelix Logo + 光影扫描动画 |
| `src/ui/widgets/chat_area.rs` | UI | 消息列表 + 打字机效果 |
| `src/ui/widgets/input_area.rs` | UI | 多行输入区（基于 ratatui-textarea） |
| `src/ui/widgets/sidebar.rs` | UI | 右侧面板：待办/通知/任务进度 |
| `src/ui/widgets/markdown.rs` | UI | Markdown 文本渲染（termimad 或自实现） |

### 需要修改
| 文件 | 修改内容 |
|------|----------|
| `src/lib.rs` | 重写：导出新模块架构（domain/infrastructure/application/ui）+ 提供 `run_tui()` 入口 |
| `Cargo.toml` | 新增依赖：`ratatui-textarea`, `termimad`（MD 渲染）, `async-trait`; 移除不必要的直接依赖 caelix-service（只在 infra/mock 中需要时引入） |

---

## 四、分步实施步骤

### 第一步：清除现有实现
1. 删除 `src/runner.rs`, `src/commands.rs`, `src/events.rs`, `src/state.rs`, `src/views.rs`
2. 清空 `src/lib.rs`（留空壳）
3. 更新 `Cargo.toml` 依赖

### 第二步：搭建领域层 (domain)
1. 创建 `domain/mod.rs` 及各子模块
2. 定义 `Message { id, role, content, created_at, is_streaming }`
3. 定义 `MessageRole` (User, Assistant, System)
4. 定义 `Task { id, title, status, progress, ... }` + `TaskStatus` 枚举
5. 定义 `Notification { id, level, content, timestamp }` + `NotificationLevel`
6. 定义 `Session { id, title, created_at }`
7. 定义常量（动画帧间隔、打字机速度等）

### 第三步：搭建基础设施层 (infrastructure)
1. 创建 `traits.rs` 定义：
   - `ChatService`: `send_message(&self, input: &str) -> Result<Message>` / `stream_message(...)`
   - `TaskService`: `list_tasks(&self) -> Result<Vec<Task>>`
   - `NotificationService`: `list_notifications(&self) -> Result<Vec<Notification>>`
2. 创建 Mock 实现，返回硬编码假数据（模拟 AI 回复、待办列表等）

### 第四步：搭建应用服务层 (application)
1. 创建 `AppService`，持有：
   - `messages: Vec<Message>`（对话历史）
   - `tasks: Vec<Task>`
   - `notifications: Vec<Notification>`
   - `chat_service: Box<dyn ChatService>`
   - `task_service: Box<dyn TaskService>`
   - ...
2. 实现业务方法：
   - `send_user_message(&mut self, content: String)` → 添加用户消息 + 调用 ChatService
   - `append_stream_chunk(&mut self, message_id, chunk)` → 打字机数据追加
   - `get_messages(&self) -> &[Message]`
   - `get_tasks(&self) -> &[Task]`
   - `get_notifications(&self) -> &[Notification]`
   - Mock 阶段：`send_user_message` 直接调用 Mock ChatService 的同步/模拟流式返回

### 第五步：搭建 UI 层骨架
1. 创建 `ui/theme.rs` 定义配色（Dark 主题，参考 VSCode/Codex 配色）
2. 创建 `ui/event.rs` 定义 `UiEvent`（Key, Tick, Quit, Resize）
3. 创建 `ui/app.rs` 定义 `TuiApp`：
   - 持有 `AppService`
   - UI 状态：`mode (Splash | Input | Chat)`, `cursor`, `scroll_offset`, `typewriter_state`
   - `handle_event()`: 只调用 AppService 的方法
4. 创建 `ui/renderer.rs` 调度渲染

### 第六步：实现启动页 (Splash)
1. `ui/widgets/splash.rs`:
   - 居中显示 "Caelix" ASCII/Figlet Logo
   - 光影扫描效果：一条渐变光带从左到右扫过 Logo（基于 tick 动画）
   - 3-5 秒后自动进入输入模式，或按任意键跳过

### 第七步：实现输入界面
1. 集成 `ratatui-textarea`：
   - 居中的输入框区域（占屏幕 60% 宽，10 行高）
   - 支持 Enter 换行，Ctrl+Enter 发送
   - 下方提示文字："Enter 换行 · Ctrl+Enter 发送"
2. 发送后：`TuiApp` 调用 `app_service.send_user_message(...)`，切换到 Chat 模式

### 第八步：实现核心聊天界面
1. **布局**：左右分栏 `Constraint::Percentage(80)` | `Constraint::Percentage(20)`
2. **左侧 (80%)** 再垂直分栏：
   - 上：`chat_area.rs` 消息渲染区（Min 占满剩余空间）
   - 下：`input_area.rs` 输入区（固定 5-8 行高）
3. **右侧 (20%)** `sidebar.rs` 垂直堆叠：
   - 待办列表 (Tasks)
   - 通知消息 (Notifications)  
   - 任务进度 (Progress bars)
4. **消息渲染顺序**：最新消息在**最上方**（与微信/豆包一致，最新输入置顶）
5. **打字机效果**：
   - `AppService` 在 mock 阶段按字符增量 `append_stream_chunk`
   - UI 层每次 tick 检查是否有新内容，有则重绘
6. **Markdown 渲染**：
   - `ui/widgets/markdown.rs` 基于 `termimad` 或自实现简化版
   - 支持标题、粗体、斜体、代码块、列表

### 第九步：整合与打磨
1. `lib.rs` 暴露 `pub async fn run_tui() -> Result<()>` 
2. 确保 `caelix-bin/src/main.rs` 的 `--tui` 路径调用新的 `run_tui`
3. 运行 `cargo build -p caelix-tui` 验证编译
4. 运行 `cargo run -p caelix-bin -- --tui` 验证交互

---

## 五、新增依赖

需要在 `caelix-tui/Cargo.toml` 中添加：
```toml
[dependencies]
# UI 框架（已有）
ratatui.workspace = true
crossterm.workspace = true

# 异步（已有）
tokio.workspace = true
futures.workspace = true

# 文本输入组件
ratatui-textarea = "0.7"

# Markdown 终端渲染
termimad = "0.31"

# Trait 抽象
async-trait.workspace = true

# 工具
chrono.workspace = true
thiserror.workspace = true
```

---

## 六、风险与注意事项

| 风险 | 应对措施 |
|------|----------|
| ratatui-textarea API 不熟悉 | 先写最小示例验证，必要时降级为自实现简单 TextArea |
| termimad 与 ratatui 集成问题 | termimad 可生成 text/cow，再用 Paragraph 渲染；若有兼容问题则自实现简化 MD 解析器 |
| 打字机动画与输入冲突 | 使用独立的 tick 事件驱动动画，输入事件不触发动画帧 |
| Mock 与真实后端切换 | 通过 feature flag 或构造函数参数控制，`AppService::new_mock()` vs `AppService::new_real(...)` |
| 最新消息在最上方与滚动条冲突 | 反转渲染顺序，messages.last() 渲染在列表顶部，scroll_offset 从 0 开始代表最新 |

---

## 七、验证标准

1. ✅ `cargo build -p caelix-tui` 通过
2. ✅ `cargo build`（整个 workspace）通过
3. ✅ 启动 TUI 能看到 Caelix Logo + 光影动画
4. ✅ 按任意键/动画结束后进入输入界面
5. ✅ 输入框支持 Enter 换行，Ctrl+Enter 发送
6. ✅ 发送后进入聊天界面，左 80% 右 20% 布局
7. ✅ 左侧显示消息（新消息在上），AI 回复有打字机效果
8. ✅ 右侧显示待办/通知/进度（Mock 数据）
9. ✅ UI 层代码不包含 `reqwest::`, `std::fs::`, `std::io::File` 等关键字
10. ✅ 所有业务操作均通过 `AppService` 公开方法完成
