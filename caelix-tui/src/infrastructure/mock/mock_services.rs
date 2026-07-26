use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::Utc;

use crate::domain::{
    Notification, NotificationId, NotificationLevel, Task, TaskId, TaskProgress,
    TaskStatus,
};

use super::super::traits::{ChatService, NotificationService, ServiceResult, TaskService};

pub struct MockServices {
    reply_count: AtomicUsize,
}

impl MockServices {
    pub fn new() -> Self {
        Self {
            reply_count: AtomicUsize::new(0),
        }
    }
}

impl Default for MockServices {
    fn default() -> Self {
        Self::new()
    }
}

fn first_reply(user_input: &str) -> String {
    format!(
        "收到你的消息：**\"{}\"**\n\n\
         这是一条 **Mock** 回复，用于演示 TUI 界面。\n\n\
         ## 功能演示\n\n\
         - 支持 **粗体** 和 *斜体* 文本\n\
         - 支持 `代码片段` 高亮\n\
         - 支持有序列表\n\n\
         ```rust\n\
         fn hello() {{\n\
             println!(\"Hello, Caelix!\");\n\
         }}\n\
         ```\n\n\
         > 这是一段引用文字，展示 Markdown 的引用块效果。\n\n\
         你可以继续输入其他内容，我会用打字机效果逐字回复。",
        user_input
    )
}

fn second_reply() -> String {
    let mut s = String::new();
    s.push_str("让我分析一下你的需求。\n\n");
    s.push_str("根据你的描述，我理解你希望实现一个多步骤的复杂任务。这个任务涉及多个文件的搜索、代码分析、以及相应的修改。我会按照标准的工作流程一步步执行，确保每一步都正确且可追溯。\n\n");

    s.push_str("## 🔍 第一步：grep 搜索相关文件\n\n");
    s.push_str("我正在使用 grep 在整个代码库中搜索与 'todo' 相关的所有定义和引用：\n\n");
    s.push_str("```\ngrep -rn \"todo\\|TODO\\|task\\|Task\" --include=\"*.rs\" --include=\"*.toml\" src/\n```\n\n");
    s.push_str("找到以下相关文件：\n");
    s.push_str("- `src/domain/task.rs` - 任务领域模型定义（Task, TaskId, TaskStatus, TaskProgress）\n");
    s.push_str("- `src/domain/message.rs` - 消息领域模型定义（Message, MessageRole, MessageId）\n");
    s.push_str("- `src/application/app_service.rs` - 应用服务层，负责任务调度和状态管理\n");
    s.push_str("- `src/infrastructure/traits.rs` - 基础设施服务接口定义（ChatService, TaskService, NotificationService）\n");
    s.push_str("- `src/ui/app.rs` - UI 状态机，处理用户输入和界面状态流转\n");
    s.push_str("- `src/ui/renderer.rs` - 渲染器，负责将状态绘制到终端\n");
    s.push_str("- `caelix-task/src/lib.rs` - 任务执行引擎核心\n\n");

    s.push_str("## 📝 第二步：需求分析\n\n");
    s.push_str("经过初步分析，我发现以下几个关键点需要注意：\n\n");
    s.push_str("1. **滚动模型**：当前使用的是消息级索引滚动，但单条消息可能很长，需要支持行级滚动\n");
    s.push_str("2. **光标定位**：虚拟光标需要精确定位到渲染后的字符，而不是原始文本字符\n");
    s.push_str("3. **自动滚动**：当 AI 正在生成回复时，如果用户没有手动翻阅，应自动跟随最新内容\n");
    s.push_str("4. **边界保护**：所有坐标计算必须有严格的边界检查，防止 panic\n");
    s.push_str("5. **性能优化**：滚动计算应尽量高效，避免每帧全量重绘\n\n");

    s.push_str("## ✏️ 第三步：文件修改\n\n");
    s.push_str("下面是我对 `src/domain/task.rs` 的修改：\n\n");
    s.push_str("```diff\n--- a/src/domain/task.rs\n+++ b/src/domain/task.rs\n@@ -42,6 +42,7 @@\n pub struct Task {\n     pub id: TaskId,\n     pub title: String,\n+    pub priority: Priority,\n     pub status: TaskStatus,\n     pub progress: Option<TaskProgress>,\n     pub created_at: DateTime<Utc>,\n@@ -55,6 +56,15 @@\n     pub updated_at: DateTime<Utc>,\n }\n \n+#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]\n+pub enum Priority {\n+    Low,\n+    Medium,\n+    High,\n+    Critical,\n+}\n+\n impl Default for Task {\n     fn default() -> Self {\n         Self {\n             id: TaskId(0),\n             title: String::new(),\n+            priority: Priority::Medium,\n             status: TaskStatus::Pending,\n             progress: None,\n```\n\n");

    s.push_str("接下来是对 `src/application/app_service.rs` 的修改：\n\n");
    s.push_str("```diff\n--- a/src/application/app_service.rs\n+++ b/src/application/app_service.rs\n@@ -88,6 +88,7 @@\n         let task = Task {\n             id: TaskId(new_id),\n             title: title.to_string(),\n+            priority: Priority::Medium,\n             status: TaskStatus::Pending,\n             progress: None,\n             created_at: Utc::now(),\n```\n\n");

    s.push_str("## 🔬 第四步：测试验证\n\n");
    s.push_str("我运行了完整的测试套件来验证修改：\n\n");
    s.push_str("```bash\n$ cargo test -p caelix-tui\n   Compiling caelix-tui v0.1.0\n    Finished test [unoptimized + debuginfo] in 2.34s\n     Running unittests src/lib.rs\n\ntest result: ok. 12 passed; 0 failed; 0 ignored; 0 measured\n```\n\n");
    s.push_str("所有测试均已通过，没有回归问题。\n\n");

    s.push_str("## 📊 影响范围分析\n\n");
    s.push_str("本次修改涉及以下模块：\n\n");
    s.push_str("| 模块 | 影响程度 | 说明 |\n");
    s.push_str("|------|---------|------|\n");
    s.push_str("| domain/task.rs | 高 | 新增 Priority 枚举和字段 |\n");
    s.push_str("| application/app_service.rs | 中 | 创建任务时设置默认优先级 |\n");
    s.push_str("| ui/widgets/sidebar.rs | 低 | 任务显示增加优先级标识 |\n");
    s.push_str("| caelix-task | 低 | 任务调度器可根据优先级排序 |\n\n");

    s.push_str("## 🚀 后续优化建议\n\n");
    s.push_str("基于本次分析，我建议在未来版本中考虑以下优化：\n\n");
    s.push_str("1. **优先级调度**：任务执行引擎可以根据优先级自动调整执行顺序\n");
    s.push_str("2. **优先级过滤**：在侧边栏中增加按优先级筛选任务的功能\n");
    s.push_str("3. **紧急通知**：Critical 优先级的任务应触发桌面通知\n");
    s.push_str("4. **历史统计**：统计不同优先级任务的完成率和平均耗时\n");
    s.push_str("5. **自定义优先级**：允许用户自定义优先级名称和颜色\n\n");

    s.push_str("## ✅ 任务完成\n\n");
    s.push_str("已成功为 Task 结构体添加 priority 字段及相关基础设施。\n");
    s.push_str("已更新所有相关的创建、序列化和显示逻辑。\n");
    s.push_str("已通过全部单元测试和集成测试。\n");
    s.push_str("已完成代码审查，没有发现安全问题或性能隐患。\n\n");

    s.push_str("以上就是我这次的完整分析和操作结果。整个过程涉及了需求理解、代码搜索、方案设计、多文件修改、以及完整的测试验证。你还有其他需要帮助的吗？");

    s
}

#[async_trait]
impl ChatService for MockServices {
    async fn generate_reply(&self, user_input: &str) -> ServiceResult<String> {
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let count = self.reply_count.fetch_add(1, Ordering::SeqCst);
        let reply = if user_input.trim().is_empty() {
            "你好！我是 Caelix AI 助手。有什么可以帮助你的吗？".to_string()
        } else if count == 0 {
            first_reply(user_input)
        } else {
            second_reply()
        };

        Ok(reply)
    }

    async fn stream_reply(&self, user_input: &str) -> ServiceResult<Vec<String>> {
        let full = self.generate_reply(user_input).await?;
        let mut chunks = Vec::new();
        let chars: Vec<char> = full.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let end = (i + 8).min(chars.len());
            chunks.push(chars[i..end].iter().collect());
            i = end;
        }
        Ok(chunks)
    }
}

#[async_trait]
impl TaskService for MockServices {
    async fn list_tasks(&self) -> ServiceResult<Vec<Task>> {
        let now = Utc::now();
        Ok(vec![
            Task {
                id: TaskId(1),
                title: "重构 TUI 架构".to_string(),
                status: TaskStatus::Running,
                progress: Some(TaskProgress { current: 6, total: 9 }),
                created_at: now,
            },
            Task {
                id: TaskId(2),
                title: "实现打字机效果".to_string(),
                status: TaskStatus::Pending,
                progress: None,
                created_at: now,
            },
            Task {
                id: TaskId(3),
                title: "集成 Markdown 渲染".to_string(),
                status: TaskStatus::Completed,
                progress: Some(TaskProgress { current: 100, total: 100 }),
                created_at: now,
            },
            Task {
                id: TaskId(4),
                title: "修复启动页动画".to_string(),
                status: TaskStatus::Failed,
                progress: None,
                created_at: now,
            },
        ])
    }
}

#[async_trait]
impl NotificationService for MockServices {
    async fn list_notifications(&self) -> ServiceResult<Vec<Notification>> {
        let now = Utc::now();
        Ok(vec![
            Notification {
                id: NotificationId(1),
                level: NotificationLevel::Success,
                content: "会话已创建成功".to_string(),
                timestamp: now,
            },
            Notification {
                id: NotificationId(2),
                level: NotificationLevel::Info,
                content: "正在加载模型配置...".to_string(),
                timestamp: now,
            },
            Notification {
                id: NotificationId(3),
                level: NotificationLevel::Warning,
                content: "API 调用次数接近限额".to_string(),
                timestamp: now,
            },
        ])
    }
}
