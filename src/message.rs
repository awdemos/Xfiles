use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Plan 9-inspired message envelope used for all inter-agent communication.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub conversation_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sender: String,
    pub sender_ns: String,
    pub path: String,
    pub msg_type: String,
    pub data: serde_json::Value,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub quantum: Option<QuantumMetadata>,
}

impl Message {
    pub fn new(sender: impl Into<String>, path: impl Into<String>, msg_type: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: None,
            conversation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: sender.into(),
            sender_ns: "/net/unknown".into(),
            path: path.into(),
            msg_type: msg_type.into(),
            data: serde_json::Value::Null,
            headers: HashMap::new(),
            quantum: None,
        }
    }

    pub fn with_conversation(mut self, cid: Uuid) -> Self {
        self.conversation_id = cid;
        self
    }

    pub fn with_parent(mut self, pid: Uuid) -> Self {
        self.parent_id = Some(pid);
        self
    }

    pub fn with_data(mut self, data: impl Serialize) -> Self {
        self.data = serde_json::to_value(data).unwrap_or_default();
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_quantum(mut self, q: QuantumMetadata) -> Self {
        self.quantum = Some(q);
        self
    }
}

/// Carried on messages when quantum mode is active.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuantumMetadata {
    /// Probability amplitudes per endpoint id.
    pub amplitudes: HashMap<String, f64>,
    /// Selected endpoint after collapse.
    pub collapsed_to: Option<String>,
    /// Whether this message is entangled with others.
    pub entangled: bool,
    /// Entanglement group id.
    pub entanglement_id: Option<Uuid>,
}

/// Response envelope from the hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub message_id: Uuid,
    pub status: DispatchStatus,
    pub routed_to: Vec<String>,
    pub explanation: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchStatus {
    Routed,
    Queued,
    Dropped,
    Error,
}

/// Agent capability manifest sent during registration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilityManifest {
    pub agent_id: String,
    pub hostname: String,
    pub capabilities: Vec<Capability>,
    pub preferred_namespace: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Feedback event for learning loops.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedbackEvent {
    pub message_id: Uuid,
    pub endpoint_id: String,
    pub success: bool,
    pub latency_ms: u64,
    pub quality_score: Option<f32>,
    pub error_kind: Option<String>,
    pub metadata: Option<serde_json::Value>,
}
