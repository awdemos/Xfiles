use std::sync::Arc;
use xfiles::agent::AgentRegistry;
use xfiles::ai::endpoints::{AiEndpoint, EndpointType};
use xfiles::circuit::CircuitBreaker;
use xfiles::config::{Config, HubConfig, QuantumConfig, CircuitBreakerConfig, RateLimitConfig, AuthConfig};
use xfiles::event::{Event, EventEmitter, EventKind, TracingEventEmitter};
use xfiles::fs::VfsRegistry;
use xfiles::message::Message;
use xfiles::mcp::McpRegistry;
use xfiles::plumber::Plumber;
use xfiles::queue::MessageQueue;
use xfiles::router::{default_pipeline, RoutingPipeline};
use xfiles::state::StateManager;
use xfiles::store::Store;
use dashmap::DashMap;

fn test_config() -> Config {
    Config {
        hub: HubConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            max_agents: 100,
            heartbeat_interval_secs: 30,
            probe_interval_secs: 60,
            database_url: ":memory:".to_string(),
        },
        quantum: QuantumConfig::default(),
        circuit: CircuitBreakerConfig::default(),
        rate_limit: RateLimitConfig::default(),
        auth: AuthConfig::default(),
        ..Default::default()
    }
}

fn build_test_endpoints() -> Arc<DashMap<String, AiEndpoint>> {
    let map = DashMap::new();
    map.insert(
        "ep-a".into(),
        AiEndpoint {
            id: "ep-a".into(),
            name: "alpha".into(),
            url: "http://localhost:8001".into(),
            endpoint_type: EndpointType::Inference,
            weight: 1.0,
            tags: vec![],
            headers: Default::default(),
            health: Default::default(),
        },
    );
    map.insert(
        "ep-b".into(),
        AiEndpoint {
            id: "ep-b".into(),
            name: "beta".into(),
            url: "http://localhost:8002".into(),
            endpoint_type: EndpointType::Inference,
            weight: 1.0,
            tags: vec![],
            headers: Default::default(),
            health: Default::default(),
        },
    );
    Arc::new(map)
}

#[tokio::test]
async fn test_routing_pipeline_selects_endpoint() {
    let endpoints = build_test_endpoints();
    let mcp = Arc::new(McpRegistry::new());
    let plumber = Plumber::new();
    plumber.add_rule("ai", "llm_request", "ep-a", 1, None).unwrap();
    plumber.add_rule("ai", "llm_request", "ep-b", 1, None).unwrap();
    let pipeline = default_pipeline(mcp, plumber, None, None, endpoints);

    let msg = Message::new("test", "/ai", "llm_request");
    let decision = pipeline.route(&msg).await;

    assert!(decision.selected.is_some(), "pipeline should select an endpoint");
    let selected = decision.selected.unwrap();
    assert!(selected == "ep-a" || selected == "ep-b", "selected should be one of the test endpoints");
}

#[tokio::test]
async fn test_state_manager_creation_and_accessors() {
    let store = Arc::new(Store::new(":memory:").await.expect("create store"));
    let emitter = Arc::new(TracingEventEmitter::new(None));

    let state = StateManager::new(
        AgentRegistry::new(),
        build_test_endpoints(),
        VfsRegistry::new(),
        Plumber::new(),
        None,
        Arc::new(MessageQueue::new()),
        store,
        Arc::new(McpRegistry::new()),
        None,
        Arc::new(test_config()),
        emitter,
    );

    assert_eq!(state.agents().len(), 0);
    assert_eq!(state.endpoints().len(), 2);
    assert!(state.quantum().is_none());
    assert!(state.circuit().is_none());
}

#[tokio::test]
async fn test_event_logging_flow() {
    let store = Arc::new(Store::new(":memory:").await.expect("create store"));
    let sink = Arc::new(xfiles::event::StoreEventSink::new(store.clone()));
    let emitter = Arc::new(TracingEventEmitter::new(Some(sink)));

    let event = Event::new(EventKind::SystemStartup, "test", "system started");
    emitter.emit(event);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_circuit_breaker_filters_unhealthy() {
    let endpoints = build_test_endpoints();

    {
        let mut ep = endpoints.get_mut("ep-a").unwrap();
        ep.health.status = xfiles::ai::endpoints::HealthStatus::Offline;
    }

    let circuit = Arc::new(CircuitBreaker::new(3, 60, 1));
    let mcp = Arc::new(McpRegistry::new());
    let plumber = Plumber::new();
    plumber.add_rule("ai", "llm_request", "ep-a", 1, None).unwrap();
    plumber.add_rule("ai", "llm_request", "ep-b", 1, None).unwrap();
    let pipeline = default_pipeline(mcp, plumber, None, Some(circuit), endpoints.clone());

    let msg = Message::new("test", "/ai", "llm_request");
    let decision = pipeline.route(&msg).await;

    assert_eq!(decision.selected, Some("ep-b".into()), "offline endpoint should be filtered out");
}


