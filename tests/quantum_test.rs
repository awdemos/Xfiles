use std::sync::Arc;
use xfiles::ai::endpoints::{AiEndpoint, EndpointType};
use xfiles::quantum::QuantumRouter;
use xfiles::config::QuantumConfig;
use dashmap::DashMap;

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
async fn test_quantum_selects_from_candidates() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let router = QuantumRouter::new(endpoints, config, None);

    let msg = xfiles::message::Message::new("test", "/ai", "llm_request");
    let candidates = vec!["ep-a".into(), "ep-b".into()];
    let selected = router.route(&msg, &candidates).await;

    assert!(selected.is_some());
    let id = selected.unwrap();
    assert!(id == "ep-a" || id == "ep-b");
}

#[tokio::test]
async fn test_quantum_updates_on_observation() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let router = QuantumRouter::new(endpoints, config, None);

    let conv = uuid::Uuid::new_v4();
    router.observe(conv, "ep-a", true, 100);
    router.observe(conv, "ep-a", true, 200);
    router.observe(conv, "ep-a", true, 150);

    let diagnostics = router.diagnostics(conv);
    assert_eq!(diagnostics.len(), 1);
    let (id, _prob, pulls, avg_reward) = &diagnostics[0];
    assert_eq!(id, "ep-a");
    assert_eq!(*pulls, 3);
    assert!(avg_reward > &0.0);
}

#[tokio::test]
async fn test_quantum_empty_candidates_returns_none() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let router = QuantumRouter::new(endpoints, config, None);

    let msg = xfiles::message::Message::new("test", "/ai", "llm_request");
    let selected = router.route(&msg, &[]).await;

    assert!(selected.is_none());
}
