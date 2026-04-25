/// gRPC-inspired transport for Xfiles.
///
/// This module provides a structured binary protocol using MessagePack
/// for high-efficiency agent communication. It is lighter than full gRPC
/// (no protobuf compiler required) but provides similar semantics:
/// - Unary RPC: request -> response
/// - Streaming: server-side event stream
///
/// To use: enable the `grpc` feature or connect to `/grpc` on the hub.

use crate::message::Message;
use serde::{Deserialize, Serialize};

/// gRPC-style request envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcRequest {
    pub method: String, // e.g. "xfiles.Message/Send"
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>, // msgpack-encoded Message
}

/// gRPC-style response envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcResponse {
    pub status: GrpcStatus,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub trailers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GrpcStatus {
    Ok,
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

impl GrpcStatus {
    pub fn from_http_status(status: u16) -> Self {
        match status {
            200 => GrpcStatus::Ok,
            400 => GrpcStatus::InvalidArgument,
            401 => GrpcStatus::Unauthenticated,
            403 => GrpcStatus::PermissionDenied,
            404 => GrpcStatus::NotFound,
            409 => GrpcStatus::AlreadyExists,
            429 => GrpcStatus::ResourceExhausted,
            499 => GrpcStatus::Cancelled,
            500 => GrpcStatus::Internal,
            501 => GrpcStatus::Unimplemented,
            503 => GrpcStatus::Unavailable,
            504 => GrpcStatus::DeadlineExceeded,
            _ => GrpcStatus::Unknown,
        }
    }
}

/// Encode a Message into msgpack bytes.
pub fn encode_message(msg: &Message) -> anyhow::Result<Vec<u8>> {
    let bytes = rmp_serde::to_vec_named(msg)?;
    Ok(bytes)
}

/// Decode msgpack bytes into a Message.
pub fn decode_message(bytes: &[u8]) -> anyhow::Result<Message> {
    let msg = rmp_serde::from_slice(bytes)?;
    Ok(msg)
}

/// gRPC transport codec.
#[derive(Debug, Clone, Default)]
pub struct GrpcCodec;

impl GrpcCodec {
    pub fn new() -> Self {
        Self
    }

    pub fn encode_request(&self, req: &GrpcRequest) -> anyhow::Result<Vec<u8>> {
        let bytes = rmp_serde::to_vec_named(req)?;
        Ok(bytes)
    }

    pub fn decode_request(&self, bytes: &[u8]) -> anyhow::Result<GrpcRequest> {
        let req = rmp_serde::from_slice(bytes)?;
        Ok(req)
    }

    pub fn encode_response(&self, resp: &GrpcResponse) -> anyhow::Result<Vec<u8>> {
        let bytes = rmp_serde::to_vec_named(resp)?;
        Ok(bytes)
    }

    pub fn decode_response(&self, bytes: &[u8]) -> anyhow::Result<GrpcResponse> {
        let resp = rmp_serde::from_slice(bytes)?;
        Ok(resp)
    }
}
