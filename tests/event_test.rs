use std::sync::{Arc, Mutex};
use xfiles::event::{Event, EventEmitter, EventKind, EventSink, TracingEventEmitter};

#[derive(Default)]
struct RecordingSink(Mutex<Vec<uuid::Uuid>>);

#[async_trait::async_trait]
impl EventSink for RecordingSink {
    async fn write(&self, event: &Event) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(event.id);
        Ok(())
    }
}

struct FailingSink;

#[async_trait::async_trait]
impl EventSink for FailingSink {
    async fn write(&self, _event: &Event) -> anyhow::Result<()> {
        anyhow::bail!("sink failed")
    }
}

#[test]
fn emit_without_runtime_does_not_panic() {
    let sink = Arc::new(RecordingSink::default());
    let emitter = TracingEventEmitter::new(Some(sink.clone() as Arc<dyn EventSink>));
    emitter.emit(Event::new(EventKind::SystemStartup, "test", "no runtime"));
    assert!(sink.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn emit_persists_event_through_sink() {
    let sink = Arc::new(RecordingSink::default());
    let emitter = TracingEventEmitter::new(Some(sink.clone() as Arc<dyn EventSink>));
    let event = Event::new(EventKind::MessageRouted, "test", "hello");
    let id = event.id;

    emitter.emit(event);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let persisted = sink.0.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0], id);
}

#[tokio::test]
async fn emit_tolerates_failing_sink() {
    let emitter = TracingEventEmitter::new(Some(Arc::new(FailingSink) as Arc<dyn EventSink>));
    emitter.emit(Event::new(EventKind::MessageFailed, "test", "boom"));
    tokio::task::yield_now().await;
}
