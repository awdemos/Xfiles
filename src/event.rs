use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// System event severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventSeverity {
    Debug,
    Info,
    Warn,
    Error,
}

/// Categories of system events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    MessageRouted,
    MessageDelivered,
    MessageFailed,
    EndpointHealthChanged,
    CircuitBreakerTripped,
    CircuitBreakerReset,
    AgentConnected,
    AgentDisconnected,
    QuantumStateUpdated,
    McpToolCalled,
    RateLimitHit,
    SystemStartup,
    SystemShutdown,
}

/// A structured system event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub kind: EventKind,
    pub severity: EventSeverity,
    pub source: String,
    pub message: String,
    pub conversation_id: Option<Uuid>,
    pub endpoint_id: Option<String>,
    pub agent_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Event {
    pub fn new(kind: EventKind, source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            kind,
            severity: EventSeverity::Info,
            source: source.into(),
            message: message.into(),
            conversation_id: None,
            endpoint_id: None,
            agent_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_conversation(mut self, id: Uuid) -> Self {
        self.conversation_id = Some(id);
        self
    }

    pub fn with_endpoint(mut self, id: impl Into<String>) -> Self {
        self.endpoint_id = Some(id.into());
        self
    }

    pub fn with_agent(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
        self
    }
}

/// Trait for components that can emit events.
pub trait EventEmitter: Send + Sync {
    fn emit(&self, event: Event);
}

/// Trait for event sinks that persist or forward events.
#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn write(&self, event: &Event) -> anyhow::Result<()>;
}

use std::sync::Arc;
use crate::store::Store;

/// Event sink that persists events to SQLite.
pub struct StoreEventSink {
    store: Arc<Store>,
}

impl StoreEventSink {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl EventSink for StoreEventSink {
    async fn write(&self, event: &Event) -> anyhow::Result<()> {
        self.store.insert_event(event).await
    }
}

/// Simple event emitter that logs via tracing and optionally persists.
pub struct TracingEventEmitter {
    sink: Option<Arc<dyn EventSink>>,
}

impl TracingEventEmitter {
    pub fn new(sink: Option<Arc<dyn EventSink>>) -> Self {
        Self { sink }
    }
}

impl EventEmitter for TracingEventEmitter {
    fn emit(&self, event: Event) {
        match event.severity {
            EventSeverity::Debug => tracing::debug!(?event, "event"),
            EventSeverity::Info => tracing::info!(?event, "event"),
            EventSeverity::Warn => tracing::warn!(?event, "event"),
            EventSeverity::Error => tracing::error!(?event, "event"),
        }

        if let Some(ref sink) = self.sink {
            let sink = sink.clone();
            tokio::spawn(async move {
                let _ = sink.write(&event).await;
            });
        }
    }
}
