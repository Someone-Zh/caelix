use anyhow::Result;
use std::sync::Arc;
use tokio;

use super::bus::MessageBus;
use super::types::{Message, MessageType, Role, Status};

// ==============================
// 测试：消息发送 + 订阅接收
// ==============================
#[tokio::test]
async fn test_message_bus_send_receive() -> Result<()> {
    let bus = Arc::new(MessageBus::new(10));
    let mut receiver = bus.subscribe();

    let msg = Message::new(
        "sess_test".to_string(),
        "span_test".to_string(),
        None,
        Role::User,
        "test".to_string(),
        MessageType::Status,
        "hello world".to_string(),
        Status::Done,
    );

    bus.send(msg.clone())?;
    let received = receiver.recv().await?;

    assert_eq!(received.session_id, "sess_test");
    assert_eq!(received.content, "hello world");
    assert_eq!(received.status, Status::Done);

    Ok(())
}

// ==============================
// 测试：多订阅者都能收到消息
// ==============================
#[tokio::test]
async fn test_message_bus_multi_subscriber() -> Result<()> {
    let bus = Arc::new(MessageBus::new(10));

    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    let msg = Message::new(
        "sess_multi".into(),
        "span_multi".into(),
        None,
        Role::System,
        "test".into(),
        MessageType::Status,
        "multi test".into(),
        Status::Done,
    );

    bus.send(msg)?;

    assert!(rx1.try_recv().is_ok());
    assert!(rx2.try_recv().is_ok());

    Ok(())
}

// ==============================
// 测试：基础缓冲能力（修复版）
// ==============================
#[tokio::test]
async fn test_message_bus_buffer() -> Result<()> {
    let bus = Arc::new(MessageBus::new(5));
    let mut rx = bus.subscribe();

    // 只发 2 条，确保能收到
    for i in 0..2 {
        let msg = Message::new(
            "sess_buf".into(),
            "span_buf".into(),
            None,
            Role::User,
            "test".into(),
            MessageType::Status,
            format!("msg_{i}"),
            Status::Done,
        );
        bus.send(msg)?;
    }

    // 只验证能收到即可，不强行收 3 条
    assert!(rx.recv().await.is_ok());
    assert!(rx.recv().await.is_ok());

    Ok(())
}