use crate::message::{CapabilityManifest, FeedbackEvent, Message, MessageResponse};
use serde::{Deserialize, Serialize};

/// Wire protocol envelope for WebSocket and HTTP transport.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProtocolOp {
    Hello {
        manifest: CapabilityManifest,
    },
    Heartbeat,
    Message {
        #[serde(flatten)]
        msg: Message,
    },
    Response {
        #[serde(flatten)]
        resp: MessageResponse,
    },
    Feedback {
        #[serde(flatten)]
        event: FeedbackEvent,
    },
    FsRead {
        path: String,
    },
    FsWrite {
        path: String,
        data: Vec<u8>,
    },
    FsResult {
        path: String,
        data: Option<Vec<u8>>,
        error: Option<String>,
    },
    Ack {
        message_id: uuid::Uuid,
    },
    Nak {
        message_id: uuid::Uuid,
        reason: Option<String>,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Parse a JSON string into a ProtocolOp.
pub fn parse_frame(text: &str) -> anyhow::Result<ProtocolOp> {
    let op: ProtocolOp = serde_json::from_str(text)?;
    Ok(op)
}

/// Serialize a ProtocolOp to JSON string.
pub fn encode_frame(op: &ProtocolOp) -> anyhow::Result<String> {
    let text = serde_json::to_string(op)?;
    Ok(text)
}
