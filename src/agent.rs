use crate::message::CapabilityManifest;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Connected agent state.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub uuid: Uuid,
    pub hostname: String,
    pub namespace: String,
    pub manifest: CapabilityManifest,
    pub connected_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub tx: Option<mpsc::UnboundedSender<crate::net::protocol::ProtocolOp>>,
}

/// Thread-safe agent registry.
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    agents: Arc<DashMap<String, Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, agent: Agent) {
        self.agents.insert(agent.id.clone(), agent);
    }

    pub fn unregister(&self, agent_id: &str) -> Option<Agent> {
        self.agents.remove(agent_id).map(|(_, a)| a)
    }

    pub fn get(&self, agent_id: &str) -> Option<Agent> {
        self.agents.get(agent_id).map(|e| e.clone())
    }

    pub fn heartbeat(&self, agent_id: &str) {
        if let Some(mut entry) = self.agents.get_mut(agent_id) {
            entry.last_heartbeat = Utc::now();
        }
    }

    pub fn list(&self) -> Vec<Agent> {
        self.agents.iter().map(|e| e.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    pub fn stale_agents(&self, threshold_secs: i64) -> Vec<String> {
        let now = Utc::now();
        self.agents
            .iter()
            .filter(|e| {
                let last = e.value().last_heartbeat;
                (now - last).num_seconds() > threshold_secs
            })
            .map(|e| e.key().clone())
            .collect()
    }
}
