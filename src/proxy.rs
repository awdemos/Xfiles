use crate::ai::endpoints::{AiEndpoint, HealthStatus};
use crate::circuit::CircuitBreaker;
use crate::message::Message;
use crate::quantum::QuantumRouter;
use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use uuid::Uuid;

/// OpenAI-compatible chat completion request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// OpenAI-compatible response.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xfiles_routed_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xfiles_explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ProxyState {
    pub endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>,
    pub quantum: Option<Arc<QuantumRouter>>,
    pub http: reqwest::Client,
    pub default_model: String,
    pub model_aliases: std::collections::HashMap<String, String>,
    pub circuit: Option<Arc<CircuitBreaker>>,
}

/// POST /v1/chat/completions — transparent proxy with quantum routing.
pub async fn chat_completions_handler(
    State(state): State<Arc<ProxyState>>,
    Json(mut req): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // Apply model alias if the request matches a configured alias
    if req.model.is_empty() {
        req.model = state.default_model.clone();
    } else if let Some(aliased) = state.model_aliases.get(&req.model) {
        req.model = aliased.clone();
    }

    // Build an internal message for the quantum router
    let _prompt = req
        .messages
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let msg = Message::new("proxy", "/ai/inference", "llm_request")
        .with_data(&req);

    // Collect healthy endpoint candidates, respecting circuit breaker
    let candidates: Vec<String> = state
        .endpoints
        .iter()
        .filter(|e| {
            e.health.status != HealthStatus::Offline
                && state
                    .circuit
                    .as_ref()
                    .map(|c| c.allow(&e.id))
                    .unwrap_or(true)
        })
        .map(|e| e.id.clone())
        .collect();

    // Quantum routing
    let selected_id = if let Some(ref q) = state.quantum {
        q.route(&msg, &candidates)
    } else {
        candidates.first().cloned()
    };

    let Some(endpoint_id) = selected_id else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no healthy AI endpoints available"})),
        )
            .into_response();
    };

    let endpoint = match state.endpoints.get(&endpoint_id) {
        Some(ep) => ep.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "selected endpoint no longer exists"})),
            )
                .into_response();
        }
    };

    // Forward the request
    let target_url = format!("{}/v1/chat/completions", endpoint.url.trim_end_matches('/'));
    let body = serde_json::to_string(&req).unwrap_or_default();

    let mut request_builder = state
        .http
        .post(&target_url)
        .header(header::CONTENT_TYPE, "application/json");

    for (k, v) in &endpoint.headers {
        request_builder = request_builder.header(k, v);
    }

    let upstream_resp = match request_builder.body(body).send().await {
        Ok(r) => r,
        Err(e) => {
            // Observe failure
            if let Some(ref q) = state.quantum {
                q.observe(msg.conversation_id, &endpoint_id, false, start.elapsed().as_millis() as u64);
            }
            if let Some(ref c) = state.circuit {
                c.record_failure(&endpoint_id);
            }
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("upstream error: {}", e)})),
            )
                .into_response();
        }
    };

    let latency = start.elapsed().as_millis() as u64;
    let status = upstream_resp.status();

    // Observe success / circuit
    let success = status.is_success();
    if let Some(ref q) = state.quantum {
        q.observe(msg.conversation_id, &endpoint_id, success, latency);
    }
    if let Some(ref c) = state.circuit {
        if success {
            c.record_success(&endpoint_id);
        } else {
            c.record_failure(&endpoint_id);
        }
    }

    // Stream or buffer response
    if req.stream {
        // Wrap stream to observe terminal errors
        let stream = upstream_resp.bytes_stream();
        let observed_stream = ObservingStream {
            inner: stream,
            quantum: state.quantum.clone(),
            conversation_id: msg.conversation_id,
            endpoint_id: endpoint_id.clone(),
            observed: false,
            start,
        };
        let body = Body::from_stream(observed_stream);
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(body)
            .unwrap()
            .into_response()
    } else {
        match upstream_resp.json::<serde_json::Value>().await {
            Ok(mut json) => {
                // Inject xfiles metadata
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("xfiles_routed_to".into(), json!(endpoint.name));
                    obj.insert("xfiles_endpoint_id".into(), json!(endpoint_id));
                    obj.insert("xfiles_latency_ms".into(), json!(latency));
                }
                (StatusCode::OK, Json(json)).into_response()
            }
            Err(e) => (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("failed to decode upstream response: {}", e)})),
            )
                .into_response(),
        }
    }
}

/// Stream wrapper that observes failures.
struct ObservingStream<S> {
    inner: S,
    quantum: Option<Arc<QuantumRouter>>,
    conversation_id: Uuid,
    endpoint_id: String,
    observed: bool,
    start: std::time::Instant,
}

impl<S> Stream for ObservingStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(None) => {
                // Stream completed successfully
                if !this.observed {
                    this.observed = true;
                    if let Some(ref q) = this.quantum {
                        q.observe(
                            this.conversation_id,
                            &this.endpoint_id,
                            true,
                            this.start.elapsed().as_millis() as u64,
                        );
                    }
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => {
                if !this.observed {
                    this.observed = true;
                    if let Some(ref q) = this.quantum {
                        q.observe(
                            this.conversation_id,
                            &this.endpoint_id,
                            false,
                            this.start.elapsed().as_millis() as u64,
                        );
                    }
                }
                Poll::Ready(Some(Err(e)))
            }
            other => other,
        }
    }
}

/// GET /v1/models — list available models from all healthy endpoints.
pub async fn list_models_handler(
    State(state): State<Arc<ProxyState>>,
) -> impl IntoResponse {
    let mut models = Vec::new();

    for ep in state.endpoints.iter() {
        if ep.health.status == HealthStatus::Offline {
            continue;
        }
        models.push(json!({
            "id": format!("{}/{}", ep.name, ep.id),
            "object": "model",
            "owned_by": ep.name,
            "xfiles_url": ep.url,
            "xfiles_type": ep.endpoint_type.to_string(),
            "xfiles_health": match ep.health.status {
                HealthStatus::Healthy => "healthy",
                HealthStatus::Degraded => "degraded",
                HealthStatus::Offline => "offline",
            },
        }));
    }

    Json(json!({
        "object": "list",
        "data": models,
    }))
}
