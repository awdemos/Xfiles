use crate::ai::endpoints::{AiEndpoint, EndpointHealth, EndpointType, HealthStatus};
use crate::config::DiscoveryConfig;
use bollard::container::ListContainersOptions;
use bollard::Docker;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Discovers AI services running as Docker containers.
#[derive(Debug, Clone)]
pub struct DockerDiscovery {
    docker: Docker,
    endpoints: Arc<DashMap<String, AiEndpoint>>,
    #[allow(dead_code)]
    config: DiscoveryConfig,
}

impl DockerDiscovery {
    pub fn new(endpoints: Arc<DashMap<String, AiEndpoint>>, config: DiscoveryConfig) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            endpoints,
            config,
        })
    }

    pub async fn run(&self, interval: Duration, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        // Initial scan
        if let Err(e) = self.scan().await {
            tracing::warn!("docker discovery initial scan failed: {}", e);
        }

        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.scan().await {
                        tracing::warn!("docker discovery scan failed: {}", e);
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }

    pub async fn scan(&self) -> anyhow::Result<()> {
        let options = ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        };

        let containers = self.docker.list_containers(Some(options)).await?;
        let mut found_ids = Vec::new();

        for container in containers {
            let labels = container.labels.unwrap_or_default();
            
            // Look for xfiles labels
            if let Some(service_name) = labels.get("ai.xfiles.service") {
                let service_type = labels
                    .get("ai.xfiles.type")
                    .cloned()
                    .unwrap_or_else(|| "api".to_string());
                
                let port = labels
                    .get("ai.xfiles.port")
                    .and_then(|p| p.parse::<u16>().ok())
                    .or_else(|| self.extract_first_port(&container.ports))
                    .unwrap_or(8080);

                let id = container.id.unwrap_or_default();
                let short_id = id.chars().take(12).collect::<String>();
                let endpoint_id = format!("docker-{}-{}", service_name, short_id);
                found_ids.push(endpoint_id.clone());

                if !self.endpoints.contains_key(&endpoint_id) {
                    let url = format!("http://127.0.0.1:{}", port);
                    let endpoint_type = service_type.parse().unwrap_or(EndpointType::Api);

                    let ep = AiEndpoint {
                        id: endpoint_id.clone(),
                        name: format!("docker-{}", service_name),
                        url,
                        endpoint_type,
                        weight: 1.0,
                        tags: vec!["docker".into(), "auto-discovered".into()],
                        headers: Default::default(),
                        health: EndpointHealth {
                            status: HealthStatus::Healthy,
                            ..Default::default()
                        },
                    };

                    tracing::info!(
                        "discovered docker container: {} ({}) at {}",
                        service_name,
                        endpoint_id,
                        ep.url
                    );
                    self.endpoints.insert(endpoint_id, ep);
                }
            }
        }

        self.prune_stale(&found_ids);
        Ok(())
    }

    fn prune_stale(&self, found_ids: &[String]) {
        let to_remove: Vec<String> = self
            .endpoints
            .iter()
            .filter(|e| {
                e.tags.contains(&"docker".into()) && !found_ids.contains(&e.key().clone())
            })
            .map(|e| e.key().clone())
            .collect();
        for id in to_remove {
            self.endpoints.remove(&id);
            tracing::info!("pruned stale docker endpoint: {}", id);
        }
    }

    fn extract_first_port(&self, ports: &Option<Vec<bollard::models::Port>>) -> Option<u16> {
        ports.as_ref()?.first()?.public_port.map(|p| p as u16)
    }
}
