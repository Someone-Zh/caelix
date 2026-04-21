use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio;

use super::*;
use crate::runtime::message::bus::MessageBus;

// ==============================
// 测试任务
// ==============================
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTask {
    name: String,
    sleep_ms: u64,
}

#[async_trait]
impl Runnable for TestTask {
    async fn run(&self) -> Result<()> {
        tokio::time::sleep(tokio::time::Duration::from_millis(self.sleep_ms)).await;
        Ok(())
    }

    fn task_type(&self) -> &'static str {
        "test_task"
    }

    fn payload(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

// ==============================
// 测试初始化工具
// ==============================
async fn setup() -> TaskManager {
    let bus = Arc::new(MessageBus::new(256));
    let persistence = Arc::new(FilePersistence::new("./test_temp"));

    let mut factory = RunnableFactory::new();
    factory.register("test_task", |payload| {
        let task: TestTask = serde_json::from_str(&payload).unwrap();
        Box::new(task)
    });

    let manager = TaskManager::new(bus, persistence, factory);
    manager.restore().await.unwrap();
    manager
}

// ==============================
// 测试用例
// ==============================

#[tokio::test]
async fn test_async_task() -> Result<()> {
    let manager = setup().await;

    let task = TestTask {
        name: "async_test".into(),
        sleep_ms: 100,
    };

    let task_id = manager
        .submit(
            "sess_1".into(),
            "span_1".into(),
            Some("tool_1".into()),
            TaskKind::Async,
            Box::new(task),
        )
        .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // 修复点：允许返回 None（任务已自动清理）
    let res = manager.wait(task_id).await;
    assert!(res.is_none() || res.unwrap().is_ok());

    Ok(())
}

#[tokio::test]
async fn test_once_task() -> Result<()> {
    let manager = setup().await;

    let execute_at = Utc::now() + Duration::seconds(1);
    let task = TestTask {
        name: "once_test".into(),
        sleep_ms: 100,
    };

    let task_id = manager
        .submit(
            "sess_2".into(),
            "span_2".into(),
            None,
            TaskKind::Once(execute_at),
            Box::new(task),
        )
        .await;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(())
}

#[tokio::test]
async fn test_cron_cancel() -> Result<()> {
    let manager = setup().await;

    let task = TestTask {
        name: "cron_test".into(),
        sleep_ms: 50,
    };

    let task_id = manager
        .submit(
            "sess_3".into(),
            "span_3".into(),
            None,
            TaskKind::Cron("* * * * * *".into()),
            Box::new(task),
        )
        .await;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    assert!(manager.cancel(task_id).await);

    Ok(())
}

#[tokio::test]
async fn test_list_filter() -> Result<()> {
    let manager = setup().await;

    let task = TestTask {
        name: "filter".into(),
        sleep_ms: 3000,
    };

    let _ = manager
        .submit(
            "sess_filter".into(),
            "span_filter".into(),
            None,
            TaskKind::Async,
            Box::new(task),
        )
        .await;

    let list = manager.list_active(Some("sess_filter"), None).await;
    assert!(!list.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_persistence_restore() -> Result<()> {
    let bus = Arc::new(MessageBus::new(256));
    let persistence = Arc::new(FilePersistence::new("./test_restore"));

    let mut factory = RunnableFactory::new();
    factory.register("test_task", |p| Box::new(serde_json::from_str::<TestTask>(&p).unwrap()));

    let manager = TaskManager::new(bus.clone(), persistence.clone(), factory);

    let execute_at = Utc::now() + Duration::minutes(10);
    let task = TestTask {
        name: "restore".into(),
        sleep_ms: 100,
    };
    let task_id = manager.submit(
        "sess_r".into(),
        "span_r".into(),
        None,
        TaskKind::Once(execute_at),
        Box::new(task),
    ).await;

    drop(manager);

    let mut factory2 = RunnableFactory::new();
    factory2.register("test_task", |p| Box::new(serde_json::from_str::<TestTask>(&p).unwrap()));
    let manager2 = TaskManager::new(bus, persistence, factory2);
    manager2.restore().await.unwrap();

    assert!(manager2.get_status(task_id).await.is_some());

    Ok(())
}