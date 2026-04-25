use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Per-endpoint circuit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Per-endpoint circuit breaker configuration and state.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<DashMap<String, EndpointCircuit>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
    half_open_max_calls: u32,
}

#[derive(Debug, Clone)]
struct EndpointCircuit {
    state: CircuitState,
    failures: u32,
    last_failure: Option<Instant>,
    half_open_calls: u32,
}

impl Default for EndpointCircuit {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            failures: 0,
            last_failure: None,
            half_open_calls: 0,
        }
    }
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout_secs: u64, half_open_max_calls: u32) -> Self {
        Self {
            state: Arc::new(DashMap::new()),
            failure_threshold,
            recovery_timeout: Duration::from_secs(recovery_timeout_secs),
            half_open_max_calls,
        }
    }

    /// Check if a request should be allowed through for this endpoint.
    pub fn allow(&self, endpoint_id: &str) -> bool {
        let mut entry = self.state.entry(endpoint_id.to_string()).or_default();

        match entry.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last) = entry.last_failure {
                    if last.elapsed() >= self.recovery_timeout {
                        entry.state = CircuitState::HalfOpen;
                        entry.half_open_calls = 0;
                        tracing::info!("circuit half-open for endpoint {}", endpoint_id);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                if entry.half_open_calls < self.half_open_max_calls {
                    entry.half_open_calls += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a success for this endpoint.
    pub fn record_success(&self, endpoint_id: &str) {
        if let Some(mut entry) = self.state.get_mut(endpoint_id) {
            match entry.state {
                CircuitState::HalfOpen => {
                    entry.state = CircuitState::Closed;
                    entry.failures = 0;
                    entry.last_failure = None;
                    tracing::info!("circuit closed for endpoint {}", endpoint_id);
                }
                CircuitState::Closed => {
                    entry.failures = 0;
                }
                _ => {}
            }
        }
    }

    /// Record a failure for this endpoint.
    pub fn record_failure(&self, endpoint_id: &str) {
        let mut entry = self.state.entry(endpoint_id.to_string()).or_default();
        entry.failures += 1;
        entry.last_failure = Some(Instant::now());

        if entry.state == CircuitState::HalfOpen {
            entry.state = CircuitState::Open;
            tracing::warn!("circuit opened for endpoint {} (half-open failure)", endpoint_id);
        } else if entry.failures >= self.failure_threshold {
            entry.state = CircuitState::Open;
            tracing::warn!(
                "circuit opened for endpoint {} ({} failures)",
                endpoint_id,
                entry.failures
            );
        }
    }

    pub fn get_state(&self, endpoint_id: &str) -> CircuitState {
        self.state
            .get(endpoint_id)
            .map(|e| e.state)
            .unwrap_or(CircuitState::Closed)
    }

    pub fn diagnostics(&self) -> Vec<(String, CircuitState, u32)> {
        self.state
            .iter()
            .map(|e| (e.key().clone(), e.state, e.failures))
            .collect()
    }
}
