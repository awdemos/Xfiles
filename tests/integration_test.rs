use std::time::Duration;

#[tokio::test]
async fn test_agent_registry_lifecycle() {
    let registry = xfiles::agent::AgentRegistry::new();

    let agent = xfiles::agent::Agent {
        id: "test-agent".into(),
        uuid: uuid::Uuid::new_v4(),
        hostname: "testbox".into(),
        namespace: "/net/test-agent".into(),
        manifest: xfiles::message::CapabilityManifest {
            agent_id: "test-agent".into(),
            hostname: "testbox".into(),
            capabilities: vec![],
            preferred_namespace: None,
        },
        connected_at: chrono::Utc::now(),
        last_heartbeat: chrono::Utc::now(),
        tx: None,
    };

    registry.register(agent.clone());
    assert_eq!(registry.len(), 1);

    let fetched = registry.get("test-agent").unwrap();
    assert_eq!(fetched.hostname, "testbox");

    registry.unregister("test-agent");
    assert!(registry.is_empty());
}

#[tokio::test]
async fn test_agent_heartbeat_and_stale_pruning() {
    let registry = xfiles::agent::AgentRegistry::new();

    let agent = xfiles::agent::Agent {
        id: "stale-agent".into(),
        uuid: uuid::Uuid::new_v4(),
        hostname: "oldbox".into(),
        namespace: "/net/stale-agent".into(),
        manifest: xfiles::message::CapabilityManifest {
            agent_id: "stale-agent".into(),
            hostname: "oldbox".into(),
            capabilities: vec![],
            preferred_namespace: None,
        },
        connected_at: chrono::Utc::now(),
        last_heartbeat: chrono::Utc::now(),
        tx: None,
    };

    registry.register(agent);
    assert_eq!(registry.len(), 1);

    // Agent should not be stale immediately
    let stale = registry.stale_agents(1);
    assert!(stale.is_empty());

    // Wait and check again
    tokio::time::sleep(Duration::from_secs(2)).await;
    let stale = registry.stale_agents(1);
    assert_eq!(stale, vec!["stale-agent"]);
}

#[tokio::test]
async fn test_namespace_mount_unmount() {
    let vfs = xfiles::fs::VfsRegistry::new();
    let agents = xfiles::agent::AgentRegistry::new();
    let ns = xfiles::namespace::NamespaceManager::new(vfs.clone(), agents);

    let manifest = xfiles::message::CapabilityManifest {
        agent_id: "ns-test".into(),
        hostname: "ns-host".into(),
        capabilities: vec![xfiles::message::Capability {
            name: "chat".into(),
            version: "1.0".into(),
            paths: vec!["/send".into(), "/recv".into()],
            metadata: Default::default(),
        }],
        preferred_namespace: None,
    };

    let path = ns.create_namespace(&manifest);
    assert!(vfs.exists(&path));
    assert!(vfs.exists("/net/ns-test/cap/chat/version"));

    ns.remove_namespace("ns-test");
    assert!(!vfs.exists("/net/ns-test"));
}
