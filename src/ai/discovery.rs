use crate::ai::endpoints::{AiEndpoint, EndpointHealth, EndpointType, HealthStatus};
use crate::config::DiscoveryConfig;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Auto-discovers AI services running on well-known ports.
#[derive(Debug, Clone)]
pub struct DiscoveryEngine {
    client: reqwest::Client,
    endpoints: Arc<DashMap<String, AiEndpoint>>,
    config: DiscoveryConfig,
}

impl DiscoveryEngine {
    pub fn new(endpoints: Arc<DashMap<String, AiEndpoint>>, config: DiscoveryConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default(),
            endpoints,
            config,
        }
    }

    pub async fn run(&self, interval: Duration, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        // Initial scan
        self.scan().await;

        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.scan().await;
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }

    pub async fn scan(&self) {
        let mut found_ids = Vec::new();
        for port in &self.config.scan_ports {
            let url = format!("http://127.0.0.1:{}", port);
            if let Some((name, endpoint_type)) = self.identify_service(&url).await {
                let id = format!("{}-{}", name, port);
                found_ids.push(id.clone());
                if !self.endpoints.contains_key(&id) {
                    let ep = AiEndpoint {
                        id: id.clone(),
                        name: name.clone(),
                        url: url.clone(),
                        endpoint_type,
                        weight: 1.0,
                        tags: vec!["auto-discovered".into()],
                        headers: Default::default(),
                        health: EndpointHealth {
                            status: HealthStatus::Healthy,
                            ..Default::default()
                        },
                    };
                    tracing::info!("discovered AI endpoint: {} at {}", name, url);
                    self.endpoints.insert(id, ep);
                }
            }
        }
        self.prune_stale(&found_ids);
    }

    fn prune_stale(&self, found_ids: &[String]) {
        let to_remove: Vec<String> = self
            .endpoints
            .iter()
            .filter(|e| {
                e.tags.contains(&"auto-discovered".into()) && !found_ids.contains(&e.key().clone())
            })
            .map(|e| e.key().clone())
            .collect();
        for id in to_remove {
            self.endpoints.remove(&id);
            tracing::info!("pruned stale auto-discovered endpoint: {}", id);
        }
    }

    async fn identify_service(&self, url: &str) -> Option<(String, EndpointType)> {
        // Try known health endpoints and signatures
        let candidates = vec![
            (format!("{}/health", url), "generic"),
            (format!("{}/v1/health", url), "generic"),
            (format!("{}/api/health", url), "generic"),
        ];

        for (probe_url, _) in candidates {
            if let Ok(resp) = self.client.get(&probe_url).send().await {
                if resp.status().is_success() {
                    // Infer service from port in URL
                    if url.contains(":8080") {
                        return Some(("routage".into(), EndpointType::Router));
                    }
                    if url.contains(":7777") {
                        return Some(("merlin".into(), EndpointType::Router));
                    }
                    if url.contains(":8000") {
                        // Could be cowabungaai or agent-memory; default to inference
                        return Some(("local-ai".into(), EndpointType::Inference));
                    }
                    if url.contains(":11434") {
                        return Some(("ollama".into(), EndpointType::Inference));
                    }
                    if url.contains(":3000") {
                        return Some(("tensorzero".into(), EndpointType::Router));
                    }
                    return Some(("unknown".into(), EndpointType::Api));
                }
            }
        }
        None
    }
}
