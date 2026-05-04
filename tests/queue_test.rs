use xfiles::queue::MessageQueue;
use xfiles::message::Message;
use xfiles::net::protocol::ProtocolOp;

#[tokio::test]
async fn test_queue_enqueue_and_drain() {
    let queue = MessageQueue::new();
    let msg = Message::new("test", "/net/target", "test_msg");

    queue.enqueue("target-agent", msg.clone());
    assert_eq!(queue.stats().len(), 1);
    assert_eq!(queue.stats()[0].1, 1);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProtocolOp>();
    let delivered = queue.drain_to("target-agent", &tx);
    assert_eq!(delivered, 1);

    if let Ok(ProtocolOp::Message { msg: received }) = rx.try_recv() {
        assert_eq!(received.id, msg.id);
    } else {
        panic!("expected message in channel");
    }

    assert!(queue.stats().is_empty());
}

#[tokio::test]
async fn test_queue_prune_old() {
    let queue = MessageQueue::new();
    let msg = Message::new("test", "/net/target", "test_msg");

    queue.enqueue("target-agent", msg);
    // Manually set enqueued_at to the past by accessing internals... not possible.
    // Instead, just verify prune_old doesn't crash on fresh queue.
    queue.prune_old(0);
    // With 0 max_age_secs, everything should be pruned
    assert!(queue.stats().is_empty());
}
