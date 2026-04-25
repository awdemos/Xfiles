use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classification of AI endpoint types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointType {
    Router,
    Inference,
    Memory,
    Agent,
    Api,
}

impl std::fmt::Display for EndpointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointType::Router => write!(f, "router"),
            EndpointType::Inference => write!(f, "inference"),
            EndpointType::Memory => write!(f, "memory"),
            EndpointType::Agent => write!(f, "agent"),
            EndpointType::Api => write!(f, "api"),
        }
    }
}

impl std::str::FromStr for EndpointType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "router" => Ok(EndpointType::Router),
            "inference" => Ok(EndpointType::Inference),
            "memory" => Ok(EndpointType::Memory),
            "agent" => Ok(EndpointType::Agent),
            "api" => Ok(EndpointType::Api),
            _ => Err(format!("unknown endpoint type: {}", s)),
        }
    }
}

/// A known AI service endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiEndpoint {
    pub id: String,
    pub name: String,
    pub url: String,
    pub endpoint_type: EndpointType,
    pub weight: f64,
    pub tags: Vec<String>,
    pub headers: HashMap<String, String>,
    pub health: EndpointHealth,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EndpointHealth {
    pub status: HealthStatus,
    pub last_checked: chrono::DateTime<chrono::Utc>,
    pub consecutive_failures: u32,
    pub probe_latency_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Offline,
}

impl Default for EndpointHealth {
    fn default() -> Self {
        Self {
            status: HealthStatus::Healthy,
            last_checked: chrono::Utc::now(),
            consecutive_failures: 0,
            probe_latency_ms: 0,
            last_error: None,
        }
    }
}

impl AiEndpoint {
    pub fn from_config(cfg: &crate::config::AiEndpoint) -> anyhow::Result<Self> {
        let endpoint_type = cfg.endpoint_type.parse().map_err(|e: String| anyhow::anyhow!(e))?;
        Ok(Self {
            id: format!("{}-{}", cfg.name, uuid::Uuid::new_v4()),
            name: cfg.name.clone(),
            url: cfg.url.clone(),
            endpoint_type,
            weight: cfg.weight,
            tags: cfg.tags.clone(),
            headers: cfg.headers.clone(),
            health: EndpointHealth::default(),
        })
    }
}
