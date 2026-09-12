use axum::{http::StatusCode, routing::get, routing::post, Json, Router};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use xfiles::ai::{
    AiEndpoint, DiscoveryEngine, EndpointHealth, EndpointType, HealthStatus, ProbeEngine,
};
use xfiles::config::DiscoveryConfig;
use xfiles::mcp::McpClient;

fn make_endpoint(id: &str, url: &str) -> AiEndpoint {
    AiEndpoint {
        id: id.into(),
        name: id.into(),
        url: url.into(),
        endpoint_type: EndpointType::Api,
        weight: 1.0,
        tags: vec![],
        headers: HashMap::new(),
        health: EndpointHealth::default(),
    }
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

#[test]
fn test_health_json_without_offline_since_still_parses() {
    let json = r#"{
        "status": "healthy",
        "last_checked": "2026-01-01T00:00:00Z",
        "consecutive_failures": 0,
        "probe_latency_ms": 0,
        "last_error": null
    }"#;
    let health: EndpointHealth = serde_json::from_str(json).unwrap();
    assert!(health.offline_since.is_none());
}

#[test]
fn test_prune_offline_ages_out_using_offline_since() {
    let endpoints: Arc<DashMap<String, AiEndpoint>> = Arc::new(DashMap::new());

    // Offline for 20 minutes but recently probed (last_checked = now).
    // With the old last_checked-only logic this would never be pruned.
    let mut stale = make_endpoint("stale-offline", "http://127.0.0.1:9");
    stale.health.status = HealthStatus::Offline;
    stale.health.offline_since = Some(chrono::Utc::now() - chrono::Duration::seconds(1200));
    stale.health.last_checked = chrono::Utc::now();
    endpoints.insert(stale.id.clone(), stale);

    // Recently transitioned offline: must be kept.
    let mut recent = make_endpoint("recent-offline", "http://127.0.0.1:9");
    recent.health.status = HealthStatus::Offline;
    recent.health.offline_since = Some(chrono::Utc::now() - chrono::Duration::seconds(60));
    recent.health.last_checked = chrono::Utc::now();
    endpoints.insert(recent.id.clone(), recent);

    let engine = ProbeEngine::new(endpoints.clone());
    let pruned = engine.prune_offline(600);
    assert_eq!(pruned, 1);
    assert!(!endpoints.contains_key("stale-offline"));
    assert!(endpoints.contains_key("recent-offline"));
}

#[tokio::test]
async fn test_probe_marks_offline_and_sets_offline_since() {
    let app = Router::new().route(
        "/health",
        get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
    );
    let url = serve(app).await;

    let endpoints: Arc<DashMap<String, AiEndpoint>> = Arc::new(DashMap::new());
    let mut ep = make_endpoint("flaky", &url);
    ep.health.consecutive_failures = 2;
    endpoints.insert(ep.id.clone(), ep);

    let engine = ProbeEngine::new(endpoints.clone());
    engine.probe_all().await;

    let ep = endpoints.get("flaky").unwrap();
    assert_eq!(ep.health.status, HealthStatus::Offline);
    assert!(ep.health.offline_since.is_some());
    assert!(ep.health.last_error.as_ref().unwrap().contains("500"));
}

#[tokio::test]
async fn test_probe_success_clears_offline_since() {
    let app = Router::new().route(
        "/health",
        get(|| async { Json(serde_json::json!({"ok": true})) }),
    );
    let url = serve(app).await;

    let endpoints: Arc<DashMap<String, AiEndpoint>> = Arc::new(DashMap::new());
    let mut ep = make_endpoint("recovered", &url);
    ep.health.status = HealthStatus::Offline;
    ep.health.consecutive_failures = 5;
    ep.health.offline_since = Some(chrono::Utc::now() - chrono::Duration::seconds(900));
    endpoints.insert(ep.id.clone(), ep);

    let engine = ProbeEngine::new(endpoints.clone());
    engine.probe_all().await;

    let ep = endpoints.get("recovered").unwrap();
    assert_eq!(ep.health.status, HealthStatus::Healthy);
    assert_eq!(ep.health.consecutive_failures, 0);
    assert!(ep.health.offline_since.is_none());
    assert!(ep.health.last_error.is_none());
}

#[tokio::test]
async fn test_discovery_prune_keeps_docker_endpoints() {
    let endpoints: Arc<DashMap<String, AiEndpoint>> = Arc::new(DashMap::new());

    // Owned by the port scanner.
    let mut port_ep = make_endpoint("local-ai-8000", "http://127.0.0.1:8000");
    port_ep.tags = vec!["auto-discovered".into()];
    endpoints.insert(port_ep.id.clone(), port_ep);

    // Owned by DockerDiscovery: tagged both "docker" and "auto-discovered".
    // The port scan never sees these, but they must not be pruned by it.
    let mut docker_ep = make_endpoint("docker-mysvc-deadbeefcafe", "http://127.0.0.1:49153");
    docker_ep.tags = vec!["docker".into(), "auto-discovered".into()];
    endpoints.insert(docker_ep.id.clone(), docker_ep);

    let config = DiscoveryConfig {
        scan_ports: vec![1], // nothing listens here
        ..Default::default()
    };
    let engine = DiscoveryEngine::new(endpoints.clone(), config);
    engine.scan().await;

    assert!(!endpoints.contains_key("local-ai-8000"));
    assert!(endpoints.contains_key("docker-mysvc-deadbeefcafe"));
}

#[tokio::test]
async fn test_mcp_call_tool_errors_on_http_500() {
    let app = Router::new().route(
        "/mcp/tools/:tool/call",
        post(
            |axum::extract::Path(_): axum::extract::Path<String>| async {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "boom"})),
                )
            },
        ),
    );
    let url = serve(app).await;

    let client = McpClient::new(make_endpoint("mcp-bad", &url));
    let result = client.call_tool("whatever", serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_discover_tools_errors_on_http_500() {
    let app = Router::new().route(
        "/mcp/tools",
        get(|| async {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "boom"})),
            )
        }),
    );
    let url = serve(app).await;

    let client = McpClient::new(make_endpoint("mcp-bad-discover", &url));
    let result = client.discover_tools().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_call_tool_success_parses_json() {
    let app = Router::new().route(
        "/mcp/tools/:tool/call",
        post(
            |axum::extract::Path(_): axum::extract::Path<String>| async {
                Json(serde_json::json!({"result": 42}))
            },
        ),
    );
    let url = serve(app).await;

    let client = McpClient::new(make_endpoint("mcp-good", &url));
    let result = client
        .call_tool("add", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(result["result"], 42);
}
