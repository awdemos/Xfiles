use crate::ai::endpoints::{AiEndpoint, HealthStatus};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Probes AI endpoints and updates health status.
#[derive(Debug, Clone)]
pub struct ProbeEngine {
    client: reqwest::Client,
    endpoints: Arc<DashMap<String, AiEndpoint>>,
}

impl ProbeEngine {
    pub fn new(endpoints: Arc<DashMap<String, AiEndpoint>>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
            endpoints,
        }
    }

    pub async fn run(&self, interval: Duration, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.probe_all().await;
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }

    pub async fn probe_all(&self) {
        for mut entry in self.endpoints.iter_mut() {
            let endpoint = entry.value_mut();
            let start = std::time::Instant::now();
            let result = self.client
                .get(format!("{}/health", endpoint.url.trim_end_matches('/')))
                .send()
                .await;

            let latency = start.elapsed().as_millis() as u64;
            endpoint.health.last_checked = chrono::Utc::now();
            endpoint.health.probe_latency_ms = latency;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    endpoint.health.consecutive_failures = 0;
                    endpoint.health.status = HealthStatus::Healthy;
                    endpoint.health.last_error = None;
                }
                Ok(resp) => {
                    endpoint.health.consecutive_failures += 1;
                    endpoint.health.status = if endpoint.health.consecutive_failures >= 3 {
                        HealthStatus::Offline
                    } else {
                        HealthStatus::Degraded
                    };
                    endpoint.health.last_error = Some(format!("HTTP {}", resp.status()));
                }
                Err(e) => {
                    endpoint.health.consecutive_failures += 1;
                    endpoint.health.status = if endpoint.health.consecutive_failures >= 3 {
                        HealthStatus::Offline
                    } else {
                        HealthStatus::Degraded
                    };
                    endpoint.health.last_error = Some(e.to_string());
                }
            }
        }
    }

    /// Remove endpoints that have been offline for longer than max_offline_secs.
    pub fn prune_offline(&self, max_offline_secs: i64) -> usize {
        let now = chrono::Utc::now();
        let to_remove: Vec<String> = self
            .endpoints
            .iter()
            .filter(|e| {
                e.health.status == HealthStatus::Offline
                    && (now - e.health.last_checked).num_seconds() > max_offline_secs
            })
            .map(|e| e.key().clone())
            .collect();

        for id in &to_remove {
            self.endpoints.remove(id);
            tracing::info!("pruned stale offline endpoint: {}", id);
        }

        to_remove.len()
    }
}
