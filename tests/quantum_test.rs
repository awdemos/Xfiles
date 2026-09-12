use dashmap::DashMap;
use std::sync::Arc;
use xfiles::ai::endpoints::{AiEndpoint, EndpointType};
use xfiles::config::QuantumConfig;
use xfiles::quantum::{QuantumRouter, QuantumStateManager};
use xfiles::store::Store;

async fn create_test_store() -> Arc<Store> {
    Arc::new(
        Store::new(":memory:")
            .await
            .expect("failed to create test store"),
    )
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
async fn test_quantum_selects_from_candidates() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let store = create_test_store().await;
    let router = QuantumRouter::new(endpoints, config, store);

    let msg = xfiles::message::Message::new("test", "/ai", "llm_request");
    let candidates = vec!["ep-a".into(), "ep-b".into()];
    let selected = router.route(&msg, &candidates);

    assert!(selected.is_some());
    let id = selected.unwrap();
    assert!(id == "ep-a" || id == "ep-b");
}

#[tokio::test]
async fn test_quantum_updates_on_observation() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let store = create_test_store().await;
    let router = QuantumRouter::new(endpoints, config, store);

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
    let store = create_test_store().await;
    let router = QuantumRouter::new(endpoints, config, store);

    let msg = xfiles::message::Message::new("test", "/ai", "llm_request");
    let selected = router.route(&msg, &[]);

    assert!(selected.is_none());
}

#[tokio::test]
async fn test_update_bumps_conversation_last_active() {
    let manager = QuantumStateManager::new();
    let conv = uuid::Uuid::new_v4();
    manager.update(conv, "ep-a", 1.0, 0.0);

    let state = manager.get(conv).expect("conversation should exist");
    {
        let mut last_active = state.last_active.lock();
        *last_active = chrono::Utc::now() - chrono::Duration::hours(2);
    }

    manager.update(conv, "ep-a", 1.0, 0.0);
    manager.prune_old(3600);

    assert!(
        manager.get(conv).is_some(),
        "recently active conversation should survive pruning"
    );
}

#[tokio::test]
async fn test_tick_prunes_stale_conversations_and_last_endpoints() {
    let endpoints = build_test_endpoints();
    let config = QuantumConfig::default();
    let store = create_test_store().await;
    let router = QuantumRouter::new(endpoints, config, store);

    let msg = xfiles::message::Message::new("test", "/ai", "llm_request");
    let candidates = vec!["ep-a".to_string(), "ep-b".to_string()];
    router.route(&msg, &candidates);

    assert_eq!(router.conversation_count(), 1);
    assert_eq!(router.last_endpoint_count(), 1);

    let state = router
        .conversation_state(msg.conversation_id)
        .expect("conversation should exist");
    *state.last_active.lock() = chrono::Utc::now() - chrono::Duration::hours(2);

    router.tick();

    assert_eq!(router.conversation_count(), 0);
    assert_eq!(
        router.last_endpoint_count(),
        0,
        "tick should drop last_endpoint entries for pruned conversations"
    );
}
