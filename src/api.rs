//! Unified API v1 — single entry point for all Xfiles subsystems.
//!
//! This module consolidates the fragmented HTTP surface into one coherent
//! resource tree. All existing legacy routes remain for backward compat.

use crate::daemon::AppState;
use crate::message::{FeedbackEvent, Message, MessageResponse, DispatchStatus};
use crate::proxy::{chat_completions_handler, list_models_handler, ProxyState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

/// Build the /api/v1 router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        // Agents
        .route("/api/v1/agents", get(list_agents).post(register_agent))
        .route("/api/v1/agents/:id", get(get_agent).delete(unregister_agent))
        .route("/api/v1/agents/:id/messages", get(agent_messages))
        // Endpoints
        .route("/api/v1/endpoints", get(list_endpoints))
        .route("/api/v1/endpoints/:id", get(get_endpoint))
        .route("/api/v1/endpoints/:id/health", get(endpoint_health))
        // VFS
        .route("/api/v1/fs/*path", get(vfs_read).post(vfs_write).delete(vfs_delete))
        // Messages
        .route("/api/v1/messages", post(send_message).get(list_messages))
        .route("/api/v1/messages/:id", get(get_message))
        // Conversations
        .route("/api/v1/conversations", get(list_conversations))
        .route("/api/v1/conversations/:id", get(get_conversation))
        .route("/api/v1/conversations/:id/messages", get(conversation_messages))
        .route("/api/v1/conversations/:id/quantum-state", get(conversation_quantum))
        // AI Proxy
        .route("/api/v1/ai/completions", post(ai_completions))
        .route("/api/v1/ai/models", get(ai_models))
        // MCP
        .route("/api/v1/mcp/tools", get(list_mcp_tools))
        .route("/api/v1/mcp/tools/:name/call", post(call_mcp_tool))
        // Quantum
        .route("/api/v1/quantum/state", get(quantum_state))
        .route("/api/v1/quantum/feedback", post(quantum_feedback))
        // Circuit
        .route("/api/v1/circuit/state", get(circuit_state))
        // Orchestrator
        .route("/api/v1/orchestrate", post(orchestrate))
        .with_state(state)
}

// ------------------------------------------------------------------
// Agents
// ------------------------------------------------------------------

async fn list_agents(State(state): State<AppState>) -> impl IntoResponse {
    let agents: Vec<serde_json::Value> = state
        .state_manager
        .agents()
        .list()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "hostname": a.hostname,
                "namespace": a.namespace,
                "connected_at": a.connected_at,
                "last_heartbeat": a.last_heartbeat,
            })
        })
        .collect();
    Json(serde_json::json!({ "agents": agents }))
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.state_manager.agents().get(&id) {
        Some(a) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": a.id,
                "hostname": a.hostname,
                "namespace": a.namespace,
                "connected_at": a.connected_at,
                "last_heartbeat": a.last_heartbeat,
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "agent not found"})),
        ),
    }
}

async fn register_agent(State(_state): State<AppState>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "use /ws/{agent_id} for registration"})))
}

async fn unregister_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    state.state_manager.agents().unregister(&id);
    state.state_manager.vfs().unmount_agent_ns(&id);
    (StatusCode::OK, Json(serde_json::json!({"status": "unregistered"})))
}

async fn agent_messages(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not yet implemented"})))
}

// ------------------------------------------------------------------
// Endpoints
// ------------------------------------------------------------------

async fn list_endpoints(State(state): State<AppState>) -> impl IntoResponse {
    let endpoints: Vec<serde_json::Value> = state
        .state_manager
        .endpoints()
        .iter()
        .map(|e| {
            let ep = e.value();
            serde_json::json!({
                "id": ep.id,
                "name": ep.name,
                "url": ep.url,
                "endpoint_type": ep.endpoint_type.to_string(),
                "health": {
                    "status": match ep.health.status {
                        crate::ai::endpoints::HealthStatus::Healthy => "healthy",
                        crate::ai::endpoints::HealthStatus::Degraded => "degraded",
                        crate::ai::endpoints::HealthStatus::Offline => "offline",
                    },
                    "last_checked": ep.health.last_checked,
                    "latency_ms": ep.health.probe_latency_ms,
                    "consecutive_failures": ep.health.consecutive_failures,
                },
                "tags": ep.tags,
            })
        })
        .collect();
    Json(serde_json::json!({ "endpoints": endpoints }))
}

async fn get_endpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.state_manager.endpoints().get(&id) {
        Some(ep) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": ep.id,
                "name": ep.name,
                "url": ep.url,
                "endpoint_type": ep.endpoint_type.to_string(),
                "health": {
                    "status": match ep.health.status {
                        crate::ai::endpoints::HealthStatus::Healthy => "healthy",
                        crate::ai::endpoints::HealthStatus::Degraded => "degraded",
                        crate::ai::endpoints::HealthStatus::Offline => "offline",
                    },
                    "latency_ms": ep.health.probe_latency_ms,
                },
                "tags": ep.tags,
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "endpoint not found"})),
        ),
    }
}

async fn endpoint_health(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.state_manager.endpoints().get(&id) {
        Some(ep) => {
            let status = match ep.health.status {
                crate::ai::endpoints::HealthStatus::Healthy => "healthy",
                crate::ai::endpoints::HealthStatus::Degraded => "degraded",
                crate::ai::endpoints::HealthStatus::Offline => "offline",
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "endpoint_id": id,
                    "status": status,
                    "latency_ms": ep.health.probe_latency_ms,
                    "consecutive_failures": ep.health.consecutive_failures,
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "endpoint not found"})),
        ),
    }
}

// ------------------------------------------------------------------
// VFS
// ------------------------------------------------------------------

async fn vfs_read(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let vfs = state.state_manager.vfs();
    if let Some(node) = vfs.get(&path) {
        if node.is_dir() {
            let children = vfs.list(&path);
            (StatusCode::OK, Json(serde_json::json!({"path": path, "type": "dir", "children": children })))
        } else {
            let data = node.read().await;
            (StatusCode::OK, Json(serde_json::json!({"path": path, "data": String::from_utf8_lossy(&data) })))
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found", "path": path })))
    }
}

async fn vfs_write(
    State(state): State<AppState>,
    Path(path): Path<String>,
    body: String,
) -> impl IntoResponse {
    let vfs = state.state_manager.vfs();
    if let Some(node) = vfs.get(&path) {
        if node.is_dir() {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "cannot write to directory", "path": path })));
        }
        match node.write(body.into_bytes()).await {
            Ok(()) => (StatusCode::OK, Json(serde_json::json!({"path": path, "status": "written" }))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string(), "path": path }))),
        }
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found", "path": path })))
    }
}

async fn vfs_delete(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    match state.state_manager.vfs().remove(&path) {
        Some(_) => (StatusCode::OK, Json(serde_json::json!({"path": path, "status": "deleted" }))),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found", "path": path }))),
    }
}

// ------------------------------------------------------------------
// Messages
// ------------------------------------------------------------------

async fn send_message(
    State(state): State<AppState>,
    Json(msg): Json<Message>,
) -> impl IntoResponse {
    let store = state.state_manager.store().clone();
    let msg_clone = msg.clone();
    tokio::spawn(async move {
        let _ = store.insert_message(&msg_clone).await;
    });

    let decision = state.pipeline.route(&msg).await;

    if let Some(ref dest) = decision.selected {
        if dest.starts_with("/net/") {
            let parts: Vec<&str> = dest.split('/').collect();
            if parts.len() >= 3 {
                let target_agent = parts[2];
                if let Some(target) = state.state_manager.agents().get(target_agent) {
                    if let Some(target_tx) = target.tx {
                        let _ = target_tx.send(crate::net::protocol::ProtocolOp::Message { msg: msg.clone() });
                    }
                } else {
                    state.state_manager.queue().enqueue(target_agent, msg.clone());
                }
            }
        }
    }

    let resp = MessageResponse {
        message_id: msg.id,
        status: DispatchStatus::Routed,
        routed_to: decision.selected.into_iter().collect(),
        explanation: Some(decision.explanation),
        latency_ms: 0,
    };

    (StatusCode::OK, Json(resp))
}

async fn list_messages(State(_state): State<AppState>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "use /conversations/:id/messages"})))
}

async fn get_message(State(_state): State<AppState>, Path(_id): Path<String>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not yet implemented"})))
}

// ------------------------------------------------------------------
// Conversations
// ------------------------------------------------------------------

async fn list_conversations(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.state_manager.store();
    match store.list_conversations(100).await {
        Ok(convs) => {
            let conversations: Vec<serde_json::Value> = convs
                .into_iter()
                .map(|(id, count)| {
                    serde_json::json!({
                        "conversation_id": id,
                        "message_count": count,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "conversations": conversations })))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let cid = match id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid uuid"}))),
    };

    let store = state.state_manager.store();
    match store.get_messages_by_conversation(cid, 1).await {
        Ok(msgs) if !msgs.is_empty() => {
            (StatusCode::OK, Json(serde_json::json!({ "conversation_id": cid, "exists": true, "messages": msgs.len() })))
        }
        Ok(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "conversation not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn conversation_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let cid = match id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid uuid"}))),
    };

    let store = state.state_manager.store();
    match store.get_messages_by_conversation(cid, 100).await {
        Ok(msgs) => {
            let messages: Vec<serde_json::Value> = msgs
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "parent_id": m.parent_id,
                        "conversation_id": m.conversation_id,
                        "timestamp": m.timestamp,
                        "sender": m.sender,
                        "path": m.path,
                        "msg_type": m.msg_type,
                        "data": m.data,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "messages": messages })))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn conversation_quantum(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let cid = match id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid uuid"}))),
    };

    match state.state_manager.quantum() {
        Some(q) => {
            let diag = q.diagnostics(cid);
            let endpoints: Vec<serde_json::Value> = diag
                .into_iter()
                .map(|(ep, prob, pulls, avg)| {
                    serde_json::json!({
                        "endpoint_id": ep,
                        "probability": prob,
                        "pulls": pulls,
                        "avg_reward": avg,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "conversation_id": cid, "endpoints": endpoints })))
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "quantum mode disabled"}))),
    }
}

// ------------------------------------------------------------------
// AI Proxy
// ------------------------------------------------------------------

async fn ai_completions(
    State(state): State<AppState>,
    Json(req): Json<crate::proxy::ChatCompletionRequest>,
) -> impl IntoResponse {
    let proxy_state = Arc::new(ProxyState {
        endpoints: state.state_manager.endpoints().clone(),
        quantum: state.state_manager.quantum().cloned(),
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        default_model: state.state_manager.config().default_model.clone(),
        model_aliases: state.state_manager.config().model_aliases.clone(),
        circuit: state.state_manager.circuit().cloned(),
    });
    chat_completions_handler(State(proxy_state), Json(req)).await
}

async fn ai_models(State(state): State<AppState>) -> impl IntoResponse {
    let proxy_state = Arc::new(ProxyState {
        endpoints: state.state_manager.endpoints().clone(),
        quantum: state.state_manager.quantum().cloned(),
        http: reqwest::Client::new(),
        default_model: state.state_manager.config().default_model.clone(),
        model_aliases: state.state_manager.config().model_aliases.clone(),
        circuit: state.state_manager.circuit().cloned(),
    });
    list_models_handler(State(proxy_state)).await
}

// ------------------------------------------------------------------
// MCP
// ------------------------------------------------------------------

async fn list_mcp_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools: Vec<serde_json::Value> = state
        .state_manager
        .mcp()
        .discover_all()
        .await
        .into_iter()
        .flat_map(|(endpoint_id, tools)| {
            tools.into_iter().map(move |t| {
                serde_json::json!({
                    "endpoint_id": endpoint_id.clone(),
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
        })
        .collect();
    Json(serde_json::json!({ "tools": tools }))
}

async fn call_mcp_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    match state.state_manager.mcp().call_tool_by_name(&name, body).await {
        Ok(Some(result)) => (StatusCode::OK, Json(result)),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "tool not found"}))),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

// ------------------------------------------------------------------
// Quantum
// ------------------------------------------------------------------

async fn quantum_state(State(state): State<AppState>) -> impl IntoResponse {
    match state.state_manager.quantum() {
        Some(q) => {
            let diagnostics = q.all_diagnostics();
            let conversations: Vec<serde_json::Value> = diagnostics
                .into_iter()
                .map(|(id, eps)| {
                    serde_json::json!({
                        "conversation_id": id,
                        "endpoints": eps.iter().map(|(ep, prob, pulls, avg)| {
                            serde_json::json!({
                                "endpoint_id": ep,
                                "probability": prob,
                                "pulls": pulls,
                                "avg_reward": avg,
                            })
                        }).collect::<Vec<_>>(),
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "active",
                    "conversation_count": q.conversation_count(),
                    "conversations": conversations,
                })),
            )
        }
        None => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "disabled" })),
        ),
    }
}

async fn quantum_feedback(
    State(state): State<AppState>,
    Json(feedback): Json<FeedbackEvent>,
) -> impl IntoResponse {
    match state.state_manager.quantum() {
        Some(q) => {
            q.observe(
                feedback.message_id,
                &feedback.endpoint_id,
                feedback.success,
                feedback.latency_ms,
            );
            let store = state.state_manager.store().clone();
            let fb_clone = feedback.clone();
            tokio::spawn(async move {
                let _ = store.insert_feedback(&fb_clone).await;
            });
            (StatusCode::OK, Json(serde_json::json!({"status": "observed"})))
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "quantum mode disabled"}))),
    }
}

// ------------------------------------------------------------------
// Circuit
// ------------------------------------------------------------------

async fn circuit_state(State(state): State<AppState>) -> impl IntoResponse {
    match state.state_manager.circuit() {
        Some(c) => {
            let diag = c.diagnostics();
            let circuits: Vec<serde_json::Value> = diag
                .into_iter()
                .map(|(ep, st, failures)| {
                    let status = match st {
                        crate::circuit::CircuitState::Closed => "closed",
                        crate::circuit::CircuitState::Open => "open",
                        crate::circuit::CircuitState::HalfOpen => "half_open",
                    };
                    serde_json::json!({
                        "endpoint_id": ep,
                        "state": status,
                        "failures": failures,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "circuits": circuits })))
        }
        None => (StatusCode::OK, Json(serde_json::json!({ "status": "disabled" }))),
    }
}

// ------------------------------------------------------------------
// Orchestrator
// ------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrchestrateRequest {
    pub intent: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: serde_json::Value,
}

async fn orchestrate(
    State(state): State<AppState>,
    Json(req): Json<OrchestrateRequest>,
) -> axum::response::Response {
    match req.intent.as_str() {
        "send_message" => {
            let msg: Message = match serde_json::from_value(req.input.clone()) {
                Ok(m) => m,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid message: {}", e)}))).into_response(),
            };
            send_message(State(state), Json(msg)).await.into_response()
        }
        "call_tool" => {
            let tool_name = req.input.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown");
            let args = req.input.get("arguments").cloned().unwrap_or_default();
            call_mcp_tool(State(state), Path(tool_name.to_string()), Json(args)).await.into_response()
        }
        "complete_chat" => {
            let chat_req: crate::proxy::ChatCompletionRequest = match serde_json::from_value(req.input.clone()) {
                Ok(r) => r,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid chat request: {}", e)}))).into_response(),
            };
            ai_completions(State(state), Json(chat_req)).await.into_response()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unknown intent",
                "supported": ["send_message", "call_tool", "complete_chat"]
            })),
        ).into_response(),
    }
}
