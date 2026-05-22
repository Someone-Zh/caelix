// src/runtime/task/scheduler.rs
use crate::types::{TaskId, TaskKind, TaskMeta};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub task_id: TaskId,
    pub meta: TaskMeta,
    pub next_run: DateTime<Utc>,
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run
    }
}

impl Eq for ScheduledTask {}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap (earliest time first)
        other.next_run.cmp(&self.next_run)
    }
}

pub struct TaskScheduler {
    heap: Arc<tokio::sync::Mutex<BinaryHeap<ScheduledTask>>>,
    notify: Arc<Notify>,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            heap: Arc::new(tokio::sync::Mutex::new(BinaryHeap::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn schedule(&self, meta: TaskMeta) {
        if let Some(next_run) = Self::calculate_next_run(&meta.kind) {
            let task_id = meta.task_id.clone();
            let task = ScheduledTask {
                task_id,
                meta,
                next_run,
            };
            let mut heap = self.heap.lock().await;
            heap.push(task);
            self.notify.notify_one();
        }
    }

    pub async fn cancel(&self, task_id: TaskId) {
        let mut heap = self.heap.lock().await;
        // 简单的重建堆来移除元素
        let mut new_heap = BinaryHeap::new();
        while let Some(task) = heap.pop() {
            if task.task_id != task_id {
                new_heap.push(task);
            }
        }
        *heap = new_heap;
    }

    pub async fn next_ready(&self) -> Option<ScheduledTask> {
        loop {
            {
                let mut heap = self.heap.lock().await;
                let now = Utc::now();

                if let Some(task) = heap.peek() {
                    if task.next_run <= now {
                        return heap.pop();
                    } else {
                        // 计算需要等待的时间
                        let duration = task.next_run.signed_duration_since(now);
                        let notify = self.notify.clone();
                        
                        // 释放锁后等待
                        drop(heap);
                        
                        tokio::select! {
                            _ = tokio::time::sleep(duration.to_std().unwrap_or(std::time::Duration::from_secs(0))) => {},
                            _ = notify.notified() => {},
                        }
                    }
                } else {
                    // 堆为空，无限等待
                    let notify = self.notify.clone();
                    drop(heap);
                    notify.notified().await;
                }
            }
        }
    }

    pub fn calculate_next_run(kind: &TaskKind) -> Option<DateTime<Utc>> {
        match kind {
            TaskKind::Once(time) => Some(*time),
            TaskKind::Cron(expr) => {
                if let Ok(schedule) = expr.parse::<Schedule>() {
                    schedule.upcoming(Utc).next()
                } else {
                    None
                }
            }
            TaskKind::Async | TaskKind::Todo => None,  // Todo 任务不由调度器触发
        }
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}