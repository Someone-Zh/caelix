// src/runtime/task/manager.rs
use crate::runtime::message::bus::MessageBus;
use crate::runtime::message::types::{Message, MessageType, Role, Status};
use crate::runtime::task::persistence::TaskPersistence;
use crate::runtime::task::scheduler::TaskScheduler;
use crate::runtime::task::types::*;
use anyhow::Result;
use chrono::Utc; // 修复：导入 Utc
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

type TaskHandle = (
    TaskMeta,
    Option<oneshot::Sender<anyhow::Result<()>>>,
    Option<JoinHandle<()>>,
);

/// 任务通知类型
enum TaskNotificationType {
    Started,
    Completed,
    Failed,
    Progress,
}

pub struct TaskManager {
    bus: Arc<MessageBus>,
    persistence: Arc<dyn TaskPersistence>,
    factory: Arc<RunnableFactory>,
    scheduler: Arc<TaskScheduler>,
    
    // 修复：显式指定 DashMap 的泛型类型
    registry: Arc<DashMap<TaskId, TaskHandle>>,
    
    // 调度器运行句柄
    _scheduler_handle: JoinHandle<()>,
}

impl std::fmt::Debug for TaskManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskManager")
            .field("registry", &self.registry)
            .finish()
    }
}

impl TaskManager {
    pub fn new(
        bus: Arc<MessageBus>,
        persistence: Arc<dyn TaskPersistence>,
        factory: RunnableFactory,
    ) -> Self {
        let scheduler = Arc::new(TaskScheduler::new());
        let registry = Arc::new(DashMap::<TaskId, TaskHandle>::new());
        let factory = Arc::new(factory);

        // 启动调度器后台循环
        let scheduler_clone = scheduler.clone();
        let registry_clone = registry.clone();
        let bus_clone = bus.clone();
        let factory_clone = factory.clone();
        let persistence_clone = persistence.clone(); // 克隆一份给后台线程

        let _scheduler_handle = tokio::spawn(async move {
            loop {
                if let Some(scheduled) = scheduler_clone.next_ready().await {
                    let task_id = scheduled.task_id;
                    
                    // 检查任务是否还在注册表中
                    if let Some(mut handle) = registry_clone.get_mut(&task_id) {
                        let (meta, _, _) = &mut handle.value_mut();
                        meta.status = TaskStatus::Running;
                        meta.updated_at = Utc::now();
                        
                        // 尝试重建 Runnable
                        if let Some(runnable) = factory_clone.create(&meta.task_type_name, meta.task_payload.clone()) {
                            let bus = bus_clone.clone();
                            let meta = meta.clone();
                            let registry = registry_clone.clone();
                            let scheduler = scheduler_clone.clone();
                            let persistence = persistence_clone.clone(); // 传给执行函数
                            
                            // Spawn 执行
                            tokio::spawn(async move {
                                Self::execute_task_inner(runnable, meta, bus, registry, scheduler, persistence).await;
                            });
                        }
                    }
                }
            }
        });

        Self {
            bus,
            persistence,
            factory,
            scheduler,
            registry,
            _scheduler_handle,
        }
    }

    /// 从持久化存储恢复任务 (启动时调用)
    pub async fn restore(&self) -> Result<()> {
        let metas = self.persistence.load_all().await?;
        for mut meta in metas {
            // 重置状态
            meta.status = TaskStatus::Scheduled;
            
            // 重新注册 (修复：去掉未使用的 _rx)
            let (tx, _) = oneshot::channel();
            self.registry.insert(meta.task_id, (meta.clone(), Some(tx), None));
            
            // 重新调度
            self.scheduler.schedule(meta).await;
        }
        Ok(())
    }

    /// 提交新任务
    pub async fn submit(
        &self,
        session_id: String,
        span_id: String,
        tool_call_id: Option<String>,
        kind: TaskKind,
        runnable: Box<dyn Runnable>,
    ) -> TaskId {
        let task_type_name = runnable.task_type().to_string();
        let task_payload = runnable.payload();
        
        let mut meta = TaskMeta::new(
            session_id.clone(),
            span_id.clone(),
            tool_call_id,
            kind.clone(),
            task_type_name,
            task_payload,
        );

        // 修复：去掉未使用的 rx
        let (tx, _) = oneshot::channel();
        let task_id = meta.task_id;

        // 1. 存入注册表
        match kind {
            TaskKind::Async => {
                meta.status = TaskStatus::Running;
                let bus = self.bus.clone();
                let registry = self.registry.clone();
                let meta_clone = meta.clone();
                let scheduler = self.scheduler.clone();
                let persistence = self.persistence.clone();
                
                // 立即 Spawn
                let handle = tokio::spawn(async move {
                    Self::execute_task_inner(runnable, meta_clone, bus, registry, scheduler, persistence).await;
                });
                
                self.registry.insert(task_id, (meta, Some(tx), Some(handle)));
            }
            TaskKind::Once(_) | TaskKind::Cron(_) => {
                meta.status = TaskStatus::Scheduled;
                self.registry.insert(task_id, (meta.clone(), Some(tx), None));
                self.scheduler.schedule(meta.clone()).await;
                let _ = self.persistence.save(&meta).await;
            }
        }

        task_id
    }

    /// 取消任务
    pub async fn cancel(&self, task_id: TaskId) -> bool {
        if let Some((mut meta, _, opt_handle)) = self.registry.remove(&task_id).map(|(_, v)| v) {
            // 尝试 Abort
            if let Some(handle) = opt_handle {
                handle.abort();
            }
            
            // 更新状态
            meta.status = TaskStatus::Cancelled;
            self.send_status_update(&meta).await;
            
            // 清理
            self.scheduler.cancel(task_id).await;
            let _ = self.persistence.delete(&task_id.to_string()).await;
            true
        } else {
            false
        }
    }

    /// 获取状态
    pub async fn get_status(&self, task_id: TaskId) -> Option<TaskMeta> {
        self.registry.get(&task_id).map(|r| r.value().0.clone())
    }

    /// 等待任务完成
    pub async fn wait(&self, task_id: TaskId) -> Option<anyhow::Result<()>> {
        // 简化版自旋等待
        loop {
            if let Some(meta) = self.get_status(task_id).await {
                match meta.status {
                    TaskStatus::Completed => return Some(Ok(())),
                    TaskStatus::Failed(e) => return Some(Err(anyhow::anyhow!(e))),
                    TaskStatus::Cancelled => return None,
                    _ => tokio::time::sleep(tokio::time::Duration::from_millis(100)).await,
                }
            } else {
                return None;
            }
        }
    }

    /// 列出任务（支持按session过滤）
    pub async fn list_tasks(&self, filter_session: Option<&str>) -> Vec<TaskMeta> {
        self.registry
            .iter()
            .filter_map(|entry| {
                let (meta, _, _) = entry.value();
                match filter_session {
                    Some(sess_id) if meta.session_id != sess_id => None,
                    _ => Some(meta.clone()),
                }
            })
            .collect()
    }

    /// 更新任务进度
    pub async fn update_progress(&self, task_id: TaskId, progress: f32) -> bool {
        if let Some(mut entry) = self.registry.get_mut(&task_id) {
            let (meta, _, _) = entry.value_mut();
            meta.progress = Some(progress.clamp(0.0, 1.0));
            meta.updated_at = Utc::now();
            
            // 发送进度通知
            Self::send_task_notification_static(meta, TaskNotificationType::Progress, &self.bus).await;
            true
        } else {
            false
        }
    }

    // ==================== 内部辅助函数 ====================

    /// 真正的执行逻辑，提取出来避免代码重复和闭包捕获问题
    async fn execute_task_inner(
        runnable: Box<dyn Runnable>,
        mut meta: TaskMeta,
        bus: Arc<MessageBus>,
        registry: Arc<DashMap<TaskId, TaskHandle>>,
        scheduler: Arc<TaskScheduler>,
        persistence: Arc<dyn TaskPersistence>, // 修复：传入 persistence
    ) {
        let task_id = meta.task_id;
        
        // 发送开始通知
        Self::send_task_notification_static(&meta, TaskNotificationType::Started, &bus).await;
        
        let result = runnable.run().await;

        // 更新状态
        let final_status = match &result {
            Ok(_) => TaskStatus::Completed,
            Err(e) => TaskStatus::Failed(e.to_string()),
        };
        
        // 先克隆结果用于通知判断
        let is_success = result.is_ok();

        // 更新注册表
        if let Some(mut entry) = registry.get_mut(&task_id) {
            let (m, opt_tx, _) = entry.value_mut();
            m.status = final_status.clone();
            m.updated_at = Utc::now();
            meta = m.clone();
            
            // 通知等待者
            if let Some(tx) = opt_tx.take() {
                let _ = tx.send(result.map_err(|e| anyhow::anyhow!(e)));
            }
        }

        // 发送完成/失败通知
        let notif_type = if is_success {
            TaskNotificationType::Completed
        } else {
            TaskNotificationType::Failed
        };
        Self::send_task_notification_static(&meta, notif_type, &bus).await;

        // 处理后续逻辑
        match meta.kind {
            TaskKind::Async | TaskKind::Once(_) => {
                // 执行完毕，移除
                registry.remove(&task_id);
                let _ = persistence.delete(&task_id.to_string()).await;
            }
            TaskKind::Cron(_) => {
                // 重新调度下一次
                if let Some(_next_run) = TaskScheduler::calculate_next_run(&meta.kind) {
                    let mut new_meta = meta.clone();
                    new_meta.status = TaskStatus::Scheduled;
                    scheduler.schedule(new_meta).await;
                }
            }
        }
    }

    /// 静态辅助方法，避免方法名冲突
    async fn send_status_update_static(meta: &TaskMeta, bus: &Arc<MessageBus>) {
        // 只有结束或失败才发送
        let content = match &meta.status {
            TaskStatus::Completed => format!("Task {} completed", meta.task_id),
            TaskStatus::Failed(e) => format!("Task {} failed: {}", meta.task_id, e),
            _ => return,
        };

        let msg = Message::new(
            meta.session_id.clone(),
            meta.span_id.clone(),
            None,
            Role::System,
            "TaskScheduler".to_string(),
            MessageType::Status,
            content,
            Status::Done,
        );
        
        let _ = bus.send(msg);
    }

    /// 发送任务通知消息
    async fn send_task_notification_static(meta: &TaskMeta, notif_type: TaskNotificationType, bus: &Arc<MessageBus>) {
        use crate::runtime::message::types::MessageMeta;
        
        let (msg_type, content) = match notif_type {
            TaskNotificationType::Started => (
                MessageType::TaskStarted,
                format!("Task {} started", meta.task_id),
            ),
            TaskNotificationType::Completed => (
                MessageType::TaskCompleted,
                format!("Task {} completed", meta.task_id),
            ),
            TaskNotificationType::Failed => (
                MessageType::TaskFailed,
                format!("Task {} failed: {}", meta.task_id, 
                    if let TaskStatus::Failed(e) = &meta.status { e } else { "unknown" }),
            ),
            TaskNotificationType::Progress => (
                MessageType::TaskProgress,
                format!("Task {} progress: {:.0}%", meta.task_id, meta.progress.unwrap_or(0.0) * 100.0),
            ),
        };
        
        let mut msg = Message::new(
            meta.session_id.clone(),
            meta.span_id.clone(),
            None,
            Role::System,
            "TaskManager".to_string(),
            msg_type,
            content,
            Status::Done,
        );
        
        // 在meta中记录task_id
        msg.meta = Some(MessageMeta {
            latency_ms: None,
            tokens_used: None,
            version: None,
            task_id: Some(meta.task_id.to_string()),
        });
        
        let _ = bus.send(msg);
    }

    /// 成员方法，供外部调用
    async fn send_status_update(&self, meta: &TaskMeta) {
        Self::send_status_update_static(meta, &self.bus).await;
    }
}