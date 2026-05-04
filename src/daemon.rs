use crate::agent::AgentRegistry;
use crate::ai::{AiEndpoint, DiscoveryEngine, ProbeEngine};
use crate::auth::{AuthConfig, api_key_middleware};
use crate::circuit::CircuitBreaker;
use crate::config::Config;
use crate::docker::DockerDiscovery;
use crate::fs::VfsRegistry;
use crate::grpc::{GrpcCodec, GrpcResponse, GrpcStatus, decode_message};
use crate::mcp::McpRegistry;
use crate::net::transport::{TransportState, handle_socket};
use crate::plumber::Plumber;
use crate::proxy::{chat_completions_handler, list_models_handler, ProxyState};
use crate::quantum::QuantumRouter;
use crate::queue::MessageQueue;
use crate::ratelimit::{RateLimiter, rate_limit_middleware};
use crate::router::{RoutingPipeline, default_pipeline};
use crate::state::StateManager;
use crate::store::Store;
use axum::{
    body::Body,
    extract::{Path, State},
    extract::ws::WebSocketUpgrade,
    http::{StatusCode, header},
    middleware::from_fn_with_state,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Shared application state for HTTP handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub state_manager: StateManager,
    pub pipeline: Arc<RoutingPipeline>,
    pub proxy: Arc<ProxyState>,
    pub transport: Arc<TransportState>,
}

/// Build and run the Xfiles daemon.
pub async fn run(config: Config) -> anyhow::Result<()> {
    // Setup tracing / OpenTelemetry
    let _otel_provider = crate::telemetry::init_telemetry("xfiles");
    // If telemetry init did not set up a subscriber, fall back to default
    if _otel_provider.is_none() {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "xfiles=debug,tower_http=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Metrics
    let _recorder_handle = PrometheusBuilder::new().install_recorder().ok();

    // SQLite store - now required for data integrity
    let store = Arc::new(Store::new(&config.hub.database_url).await.map_err(|e| {
        anyhow::anyhow!("SQLite store failed to initialize: {}. Persistence is required.", e)
    })?);
    tracing::info!("SQLite store initialized at {}", config.hub.database_url);

    // Core components
    let vfs = VfsRegistry::new();
    let agents = AgentRegistry::new();
    let plumber = Plumber::new();
    plumber.load_from_config(&config.plumber.rules)?;

    let queue = Arc::new(MessageQueue::new());

    // AI endpoints
    let endpoints: Arc<DashMap<String, AiEndpoint>> = Arc::new(DashMap::new());
    for ep_cfg in &config.ai.endpoints {
        match AiEndpoint::from_config(ep_cfg) {
            Ok(ep) => {
                tracing::info!("registered endpoint: {} at {}", ep.name, ep.url);
                endpoints.insert(ep.id.clone(), ep);
            }
            Err(e) => {
                tracing::warn!("failed to register endpoint {}: {}", ep_cfg.name, e);
            }
        }
    }

    // MCP registry
    let mut mcp_registry = McpRegistry::new();
    for ep in endpoints.iter() {
        mcp_registry.register(ep.value().clone());
    }
    // Discover and index tools asynchronously
    mcp_registry.discover_and_index().await;
    let mcp = Arc::new(mcp_registry);

    // Quantum router
    let quantum = if config.quantum.enabled {
        let q = Arc::new(QuantumRouter::new(
            endpoints.clone(),
            config.quantum.clone(),
            store.clone(),
        ));
        q.load_from_store().await;
        Some(q)
    } else {
        None
    };

    // Circuit breaker
    let circuit = if config.circuit.enabled {
        Some(Arc::new(CircuitBreaker::new(
            config.circuit.failure_threshold,
            config.circuit.recovery_timeout_secs,
            config.circuit.half_open_max_calls,
        )))
    } else {
        None
    };

    let event_sink = Arc::new(crate::event::StoreEventSink::new(store.clone()));
    let emitter = Arc::new(crate::event::TracingEventEmitter::new(Some(event_sink)));

    // Build unified state manager
    let state_manager = StateManager::new(
        agents.clone(),
        endpoints.clone(),
        vfs.clone(),
        plumber.clone(),
        quantum.clone(),
        queue.clone(),
        store.clone(),
        mcp.clone(),
        circuit.clone(),
        Arc::new(config.clone()),
        emitter,
    );

    // Build unified routing pipeline
    let pipeline = Arc::new(default_pipeline(
        mcp.clone(),
        plumber.clone(),
        quantum.clone(),
        circuit.clone(),
        endpoints.clone(),
    ));

    let transport = Arc::new(TransportState {
        agents: agents.clone(),
        vfs: vfs.clone(),
        plumber: plumber.clone(),
        quantum: quantum.clone(),
        queue: queue.clone(),
    });

    let proxy = Arc::new(ProxyState {
        endpoints: endpoints.clone(),
        quantum: quantum.clone(),
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?,
        default_model: config.default_model.clone(),
        model_aliases: config.model_aliases.clone(),
        circuit: circuit.clone(),
    });

    let app_state = AppState {
        state_manager: state_manager.clone(),
        pipeline: pipeline.clone(),
        proxy: proxy.clone(),
        transport: transport.clone(),
    };

    // Auth config
    let auth_config = Arc::new(AuthConfig {
        api_key: config.auth.api_key.clone(),
        agent_token: config.auth.agent_token.clone(),
    });

    // Rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(
        config.rate_limit.max_requests,
        config.rate_limit.window_secs,
    ));

    // Unified API v1 router
    let api_router = crate::api::build_router(app_state.clone());

    // Build router
    let mut app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/fs/*path", get(fs_read_handler).post(fs_write_handler))
        .route("/msg", post(msg_handler))
        .route("/v1/models", get(proxy_models_handler))
        .route("/v1/chat/completions", post(proxy_chat_handler))
        .route("/quantum/state", get(quantum_state_handler))
        .route("/quantum/feedback", post(quantum_feedback_handler))
        .route("/circuit/state", get(circuit_state_handler))
        .route("/grpc", post(grpc_handler))
        .route("/agents", get(agents_handler))
        .route("/endpoints", get(endpoints_handler))
        .route("/mcp/tools", get(mcp_tools_handler))
        .route("/conversations", get(conversations_handler))
        .route("/conversations/:id/messages", get(conversation_messages_handler))
        .route("/conversations/:id/quantum-state", get(conversation_quantum_handler))
        .route("/ws/{agent_id}", get(ws_handler_wrapped))
        .nest("/api/v1", api_router)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::timeout::TimeoutLayer::new(Duration::from_secs(30)))
        .with_state(app_state.clone());

    // Apply rate limiting if enabled
    if config.rate_limit.enabled {
        app = app.layer(from_fn_with_state(rate_limiter.clone(), rate_limit_middleware));
    }

    // Apply auth middleware
    app = app.layer(from_fn_with_state(auth_config.clone(), api_key_middleware));

    // Shutdown channels
    let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
    let (probe_shutdown_tx, probe_shutdown_rx) = tokio::sync::watch::channel(false);
    let (discovery_shutdown_tx, discovery_shutdown_rx) = tokio::sync::watch::channel(false);
    let (docker_shutdown_tx, docker_shutdown_rx) = tokio::sync::watch::channel(false);

    // Background: health probes
    let endpoints_probe = endpoints.clone();
    let probe_handle = if !endpoints.is_empty() {
        let probe_engine = ProbeEngine::new(endpoints_probe.clone());
        let interval = Duration::from_secs(config.hub.probe_interval_secs);
        Some(tokio::spawn(async move {
            probe_engine.run(interval, probe_shutdown_rx).await;
        }))
    } else {
        None
    };

    // Background: probe pruning (remove long-offline endpoints)
    let endpoints_prune = endpoints.clone();
    let prune_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(300));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let engine = ProbeEngine::new(endpoints_prune.clone());
            let pruned = engine.prune_offline(600);
            if pruned > 0 {
                tracing::info!("pruned {} offline endpoints", pruned);
            }
        }
    });

    // Background: port discovery
    let discovery_handle = {
        let discovery_engine = DiscoveryEngine::new(endpoints.clone(), config.discovery.clone());
        let interval = Duration::from_secs(config.discovery.scan_interval_secs);
        tokio::spawn(async move {
            discovery_engine.run(interval, discovery_shutdown_rx).await;
        })
    };

    // Background: docker discovery
    let docker_handle = if config.discovery.docker_enabled {
        match DockerDiscovery::new(endpoints.clone(), config.discovery.clone()) {
            Ok(docker_engine) => {
                let interval = Duration::from_secs(config.discovery.scan_interval_secs);
                Some(tokio::spawn(async move {
                    docker_engine.run(interval, docker_shutdown_rx).await;
                }))
            }
            Err(e) => {
                tracing::warn!("docker discovery not available: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Background: quantum maintenance
    let quantum_handle = quantum.clone().map(|q| {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                q.tick();
            }
        })
    });

    // Background: heartbeat cleanup
    let agents_cleanup = agents.clone();
    let heartbeat_interval = Duration::from_secs(config.hub.heartbeat_interval_secs);
    let cleanup_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let stale = agents_cleanup.stale_agents(120);
            for id in stale {
                tracing::info!("pruning stale agent: {}", id);
                agents_cleanup.unregister(&id);
            }
        }
    });

    // Background: queue pruning
    let queue_prune = queue.clone();
    let queue_cleanup = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            queue_prune.prune_old(3600);
        }
    });

    // TLS setup
    let tls_config = crate::tls::TlsAppConfig {
        enabled: config.tls.enabled,
        cert_path: config.tls.cert_path.clone(),
        key_path: config.tls.key_path.clone(),
        client_ca_path: config.tls.client_ca_path.clone(),
    }
    .to_tls_config()?;

    let server_future: tokio::task::JoinHandle<anyhow::Result<()>> = if let Some(tls) = tls_config {
        let rustls_config = crate::tls::build_tls_config(&tls)?;
        let rustls_config = axum_server::tls_rustls::RustlsConfig::from_config(
            std::sync::Arc::new(rustls_config)
        );
        let bind_addr = config.hub.bind_addr;
        tracing::info!("Xfiles listening on {} (TLS enabled)", bind_addr);
        if tls.client_ca_path.is_some() {
            tracing::info!("mTLS client certificate verification enabled");
        }
        tokio::spawn(async move {
            axum_server::bind_rustls(bind_addr, rustls_config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .map_err(|e| anyhow::anyhow!("server error: {}", e))?;
            Ok(())
        })
    } else {
        let listener = TcpListener::bind(config.hub.bind_addr).await?;
        tracing::info!("Xfiles listening on {}", config.hub.bind_addr);
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .map_err(|e| anyhow::anyhow!("server error: {}", e))?;
            Ok(())
        })
    };

    // Graceful shutdown
    let _shutdown_task = tokio::spawn(async move {
        let ctrl_c = async {
            let _ = signal::ctrl_c().await;
        };

        #[cfg(unix)]
        let terminate = async {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            sigterm.recv().await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => tracing::info!("Received SIGINT, shutting down gracefully"),
            _ = terminate => tracing::info!("Received SIGTERM, shutting down gracefully"),
        }

        let _ = shutdown_tx.send(true);
        let _ = probe_shutdown_tx.send(true);
        let _ = discovery_shutdown_tx.send(true);
        let _ = docker_shutdown_tx.send(true);
    });

    server_future.await??;

    tracing::info!("Server stopped, waiting for background tasks...");
    let _ = tokio::time::timeout(Duration::from_secs(5), cleanup_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), prune_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), queue_cleanup).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), discovery_handle).await;
    if let Some(h) = probe_handle {
        let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
    }
    if let Some(h) = docker_handle {
        let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
    }
    if let Some(h) = quantum_handle {
        let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
    }
    tracing::info!("Shutdown complete");

    Ok(())
}

async fn ws_handler_wrapped(
    Path(agent_id): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, agent_id, state.transport))
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn metrics_handler() -> impl IntoResponse {
    let metric_families = prometheus::gather();
    match prometheus::TextEncoder::new().encode_to_string(&metric_families) {
        Ok(text) => (StatusCode::OK, text),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn fs_read_handler(
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

async fn fs_write_handler(
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

async fn msg_handler(
    State(state): State<AppState>,
    Json(msg): Json<crate::message::Message>,
) -> impl IntoResponse {
    let store = state.state_manager.store().clone();
    let msg_clone = msg.clone();
    tokio::spawn(async move {
        let _ = store.insert_message(&msg_clone).await;
    });

    let decision = state.pipeline.route(&msg).await;

    // Try to deliver; if target agent is offline, queue it
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

    let resp = crate::message::MessageResponse {
        message_id: msg.id,
        status: crate::message::DispatchStatus::Routed,
        routed_to: decision.selected.into_iter().collect(),
        explanation: Some(decision.explanation),
        latency_ms: 0,
    };

    (StatusCode::OK, Json(resp))
}

async fn quantum_state_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

async fn proxy_models_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    list_models_handler(State(state.proxy)).await
}

async fn proxy_chat_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::proxy::ChatCompletionRequest>,
) -> impl IntoResponse {
    chat_completions_handler(State(state.proxy), Json(req)).await
}

async fn quantum_feedback_handler(
    State(state): State<AppState>,
    Json(feedback): Json<crate::message::FeedbackEvent>,
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

async fn grpc_handler(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let codec = GrpcCodec::new();
    let req = match codec.decode_request(&body) {
        Ok(r) => r,
        Err(e) => {
            let resp = GrpcResponse {
                status: GrpcStatus::InvalidArgument,
                headers: vec![],
                body: format!("decode error: {}", e).into_bytes(),
                trailers: vec![],
            };
            let encoded = codec.encode_response(&resp).unwrap_or_default();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/x-msgpack")
                .body(Body::from(encoded))
                .unwrap();
        }
    };

    let resp = match req.method.as_str() {
        "xfiles.Message/Send" => {
            match decode_message(&req.body) {
                Ok(msg) => {
                    let destinations = state.transport.plumber.route(&msg);
                    let _selected = if let Some(ref q) = state.transport.quantum {
                        q.route(&msg, &destinations)
                    } else {
                        destinations.first().cloned()
                    };
                    GrpcResponse {
                        status: GrpcStatus::Ok,
                        headers: vec![],
                        body: b"ok".to_vec(),
                        trailers: vec![],
                    }
                }
                Err(e) => GrpcResponse {
                    status: GrpcStatus::InvalidArgument,
                    headers: vec![],
                    body: format!("bad message: {}", e).into_bytes(),
                    trailers: vec![],
                },
            }
        }
        _ => GrpcResponse {
            status: GrpcStatus::Unimplemented,
            headers: vec![],
            body: b"unimplemented".to_vec(),
            trailers: vec![],
        },
    };

    let encoded = codec.encode_response(&resp).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-msgpack")
        .body(Body::from(encoded))
        .unwrap()
}

async fn agents_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

async fn endpoints_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

async fn conversation_messages_handler(
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

async fn conversation_quantum_handler(
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

async fn conversations_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

async fn circuit_state_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

async fn mcp_tools_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
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

use axum::response::Response;
