use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub hub: HubConfig,
    #[serde(default)]
    pub quantum: QuantumConfig,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub plumber: PlumberConfig,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default)]
    pub model_aliases: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub circuit: CircuitBreakerConfig,
    #[serde(default)]
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HubConfig {
    #[serde(default = "default_bind")]
    pub bind_addr: SocketAddr,
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval_secs: u64,
    #[serde(default = "default_probe")]
    pub probe_interval_secs: u64,
    #[serde(default = "default_database_url")]
    pub database_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuantumConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_decoherence")]
    pub decoherence_rate: f64,
    #[serde(default = "default_exploration")]
    pub exploration_rate: f64,
    #[serde(default = "default_entanglement")]
    pub entanglement_window: usize,
    #[serde(default = "default_min_samples")]
    pub min_samples_before_exploit: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_ports")]
    pub scan_ports: Vec<u16>,
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    #[serde(default)]
    pub docker_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiConfig {
    #[serde(default)]
    pub endpoints: Vec<AiEndpoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiEndpoint {
    pub name: String,
    pub url: String,
    #[serde(rename = "endpoint_type")]
    pub endpoint_type: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PlumberConfig {
    #[serde(default)]
    pub rules: Vec<PlumberRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlumberRule {
    pub name: String,
    pub pattern: String,
    pub destination: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub header_match: Option<std::collections::HashMap<String, String>>,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind(),
            max_agents: default_max_agents(),
            heartbeat_interval_secs: default_heartbeat(),
            probe_interval_secs: default_probe(),
            database_url: default_database_url(),
        }
    }
}

impl Default for QuantumConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            decoherence_rate: default_decoherence(),
            exploration_rate: default_exploration(),
            entanglement_window: default_entanglement(),
            min_samples_before_exploit: default_min_samples(),
        }
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            scan_ports: default_ports(),
            scan_interval_secs: default_scan_interval(),
            docker_enabled: false,
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let config_path = std::env::var("XFILES_CONFIG")
            .unwrap_or_else(|_| "xfiles.toml".into());

        if std::path::Path::new(&config_path).exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let cfg: Config = toml::from_str(&contents)?;
            return Ok(cfg);
        }

        // Fallback to defaults + env overrides
        let mut cfg = Config::default();

        if let Ok(bind) = std::env::var("XFILES_BIND") {
            cfg.hub.bind_addr = bind.parse()?;
        }
        if let Ok(max) = std::env::var("XFILES_MAX_AGENTS") {
            cfg.hub.max_agents = max.parse()?;
        }
        if let Ok(v) = std::env::var("XFILES_QUANTUM_ENABLED") {
            cfg.quantum.enabled = v.parse::<bool>().unwrap_or(true);
        }

        Ok(cfg)
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut model_aliases = std::collections::HashMap::new();
        model_aliases.insert("default".into(), default_model());
        Self {
            hub: HubConfig::default(),
            quantum: QuantumConfig::default(),
            discovery: DiscoveryConfig::default(),
            ai: AiConfig::default(),
            plumber: PlumberConfig::default(),
            default_model: default_model(),
            model_aliases,
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            circuit: CircuitBreakerConfig::default(),
            tls: TlsConfig::default(),
        }
    }
}

fn default_bind() -> SocketAddr {
    "0.0.0.0:9999".parse().unwrap()
}

fn default_max_agents() -> usize {
    100
}

fn default_heartbeat() -> u64 {
    30
}

fn default_probe() -> u64 {
    30
}

fn default_database_url() -> String {
    "sqlite:xfiles.db".into()
}

fn default_model() -> String {
    "kimi-k2.6".into()
}

fn default_true() -> bool {
    true
}

fn default_decoherence() -> f64 {
    0.05
}

fn default_exploration() -> f64 {
    0.15
}

fn default_entanglement() -> usize {
    10
}

fn default_min_samples() -> u64 {
    5
}

fn default_ports() -> Vec<u16> {
    vec![3000, 8080, 7777, 8000, 11434, 9000]
}

fn default_scan_interval() -> u64 {
    60
}

fn default_weight() -> f64 {
    1.0
}

fn default_priority() -> i32 {
    100
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub agent_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_requests")]
    pub max_requests: u64,
    #[serde(default = "default_window_secs")]
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests: default_max_requests(),
            window_secs: default_window_secs(),
        }
    }
}

fn default_max_requests() -> u64 {
    100
}

fn default_window_secs() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_recovery_timeout")]
    pub recovery_timeout_secs: u64,
    #[serde(default = "default_half_open_max")]
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: default_failure_threshold(),
            recovery_timeout_secs: default_recovery_timeout(),
            half_open_max_calls: default_half_open_max(),
        }
    }
}

fn default_failure_threshold() -> u32 {
    5
}

fn default_recovery_timeout() -> u64 {
    30
}

fn default_half_open_max() -> u32 {
    3
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub client_ca_path: Option<String>,
}
