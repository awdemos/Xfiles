use crate::agent::{Agent, AgentRegistry};
use crate::fs::VfsRegistry;
use crate::net::protocol::{encode_frame, parse_frame, ProtocolOp};
use crate::plumber::Plumber;
use crate::quantum::QuantumRouter;
use crate::queue::MessageQueue;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing;

#[derive(Debug, Clone)]
pub struct TransportState {
    pub agents: AgentRegistry,
    pub vfs: VfsRegistry,
    pub plumber: Plumber,
    pub quantum: Option<Arc<QuantumRouter>>,
    pub queue: Arc<MessageQueue>,
}

/// HTTP handler for WebSocket upgrades.
pub async fn ws_handler(
    Path(agent_id): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<TransportState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, agent_id, state))
}

pub async fn handle_socket(socket: WebSocket, agent_id: String, state: Arc<TransportState>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ProtocolOp>();

    // Spawn a task to forward from the internal channel to the WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(op) = rx.recv().await {
            match encode_frame(&op) {
                Ok(text) => {
                    if sender.send(WsMessage::Text(text)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("encode error: {}", e);
                }
            }
        }
    });

    // Handle incoming messages
    let agents = state.agents.clone();
    let vfs = state.vfs.clone();
    let plumber = state.plumber.clone();
    let quantum = state.quantum.clone();
    let queue = state.queue.clone();

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            WsMessage::Text(text) => {
                match parse_frame(&text) {
                    Ok(op) => {
                        handle_op(
                            op,
                            &agent_id,
                            &agents,
                            &vfs,
                            &plumber,
                            quantum.as_deref(),
                            &queue,
                            &tx,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::warn!("protocol parse error: {}", e);
                        let _ = tx.send(ProtocolOp::Error {
                            code: "parse_error".into(),
                            message: e.to_string(),
                        });
                    }
                }
            }
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    // Clean up
    send_task.abort();
    if let Some(agent) = agents.unregister(&agent_id) {
        tracing::info!("agent {} disconnected", agent.id);
    }
}

async fn handle_op(
    op: ProtocolOp,
    agent_id: &str,
    agents: &AgentRegistry,
    vfs: &VfsRegistry,
    plumber: &Plumber,
    quantum: Option<&QuantumRouter>,
    queue: &MessageQueue,
    tx: &mpsc::UnboundedSender<ProtocolOp>,
) {
    match op {
        ProtocolOp::Hello { manifest } => {
            let agent = Agent {
                id: agent_id.into(),
                uuid: uuid::Uuid::new_v4(),
                hostname: manifest.hostname.clone(),
                namespace: format!("/net/{}", agent_id),
                manifest,
                connected_at: chrono::Utc::now(),
                last_heartbeat: chrono::Utc::now(),
                tx: Some(tx.clone()),
            };
            agents.register(agent);
            // Drain any queued messages for this agent
            queue.drain_to(agent_id, tx);
            tracing::info!("agent {} registered", agent_id);
        }
        ProtocolOp::Heartbeat => {
            agents.heartbeat(agent_id);
        }
        ProtocolOp::Message { mut msg } => {
            msg.sender = agent_id.into();
            msg.sender_ns = format!("/net/{}", agent_id);

            // MCP tool routing takes precedence for mcp_tool_call messages
            // Note: queue is used for agent-to-agent, not MCP proxying here
            let final_dest = if msg.msg_type == "mcp_tool_call" {
                // Fall through to plumber/quantum for MCP messages over WS
                // In a full implementation we'd look up the MCP endpoint here
                let destinations = plumber.route(&msg);
                if let Some(q) = quantum {
                    q.route(&msg, &destinations).await
                } else {
                    destinations.first().cloned()
                }
            } else {
                // Plumber routing
                let destinations = plumber.route(&msg);

                // Quantum mode routing if enabled
                if let Some(q) = quantum {
                    q.route(&msg, &destinations).await
                } else {
                    destinations.first().cloned()
                }
            };

            if let Some(dest) = final_dest {
                tracing::info!("message {} routed to {}", msg.id, dest);

                // Write to destination inbox if it's an agent
                if dest.starts_with("/net/") {
                    let parts: Vec<&str> = dest.split('/').collect();
                    if parts.len() >= 3 {
                        let target_agent = parts[2];
                        if let Some(target) = agents.get(target_agent) {
                            if let Some(target_tx) = target.tx {
                                let _ = target_tx.send(ProtocolOp::Message { msg: msg.clone() });
                            }
                        } else {
                            queue.enqueue(target_agent, msg.clone());
                        }
                    }
                }
            }
        }
        ProtocolOp::FsRead { path } => {
            if let Some(node) = vfs.get(&path) {
                let data = node.read().await;
                let _ = tx.send(ProtocolOp::FsResult {
                    path,
                    data: Some(data),
                    error: None,
                });
            } else {
                let _ = tx.send(ProtocolOp::FsResult {
                    path,
                    data: None,
                    error: Some("not found".into()),
                });
            }
        }
        ProtocolOp::FsWrite { path, data } => {
            if let Some(node) = vfs.get(&path) {
                let result = node.write(data).await;
                let _ = tx.send(ProtocolOp::FsResult {
                    path,
                    data: None,
                    error: result.err().map(|e| e.to_string()),
                });
            } else {
                let _ = tx.send(ProtocolOp::FsResult {
                    path,
                    data: None,
                    error: Some("not found".into()),
                });
            }
        }
        ProtocolOp::Ack { message_id } => {
            tracing::info!("agent {} acked message {}", agent_id, message_id);
            // In a full implementation, update delivery status in store here
        }
        ProtocolOp::Nak { message_id, reason } => {
            tracing::warn!(
                "agent {} nacked message {}: {:?}",
                agent_id,
                message_id,
                reason
            );
            // In a full implementation, trigger retry or dead-letter here
        }
        _ => {}
    }
}
