use crate::ai::endpoints::AiEndpoint;
use crate::message::Message;
use crate::quantum::entanglement::EntanglementTable;
use crate::quantum::state::QuantumStateManager;
use dashmap::DashMap;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

/// Quantum-inspired router: probabilistic endpoint selection with self-learning.
#[derive(Debug, Clone)]
pub struct QuantumRouter {
    state: QuantumStateManager,
    entanglement: EntanglementTable,
    #[allow(dead_code)]
    endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>,
    config: crate::config::QuantumConfig,
    store: Option<Arc<crate::store::Store>>,
    /// conversation_id -> last selected endpoint_id
    last_endpoint: Arc<DashMap<Uuid, String>>,
}

impl QuantumRouter {
    pub fn new(
        endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>,
        config: crate::config::QuantumConfig,
        store: Option<Arc<crate::store::Store>>,
    ) -> Self {
        Self {
            state: QuantumStateManager::new(),
            entanglement: EntanglementTable::new(),
            endpoints,
            config,
            store,
            last_endpoint: Arc::new(DashMap::new()),
        }
    }

    /// Load persisted quantum state from the store.
    pub async fn load_from_store(&self) {
        let Some(ref _store) = self.store else { return };
        // We can't enumerate all conversations efficiently from the current schema,
        // so this is a hook for future optimization. For now, state rebuilds lazily.
        tracing::info!("quantum state lazy-loaded from store (future: eager restore)");
    }

    pub fn conversation_count(&self) -> usize {
        self.state.conversation_count()
    }

    pub fn all_diagnostics(&self) -> Vec<(Uuid, Vec<(String, f64, u64, f64)>)> {
        self.state.all_diagnostics()
    }

    /// Route a message to a destination using quantum-inspired selection.
    pub async fn route(&self, msg: &Message, candidates: &[String]) -> Option<String> {
        if candidates.is_empty() {
            return None;
        }

        let conv = self.state.get_or_create(msg.conversation_id);

        // Build distribution over candidates
        let mut distribution: Vec<(String, f64)> = candidates
            .iter()
            .map(|c| {
                let state = conv.get_or_insert(c);
                let prob = if state.pulls == 0 {
                    // Uniform prior for unseen endpoints
                    1.0 / candidates.len() as f64
                } else {
                    state.amplitude.probability()
                };
                (c.clone(), prob)
            })
            .collect();

        // Re-normalize
        let sum: f64 = distribution.iter().map(|(_, p)| p).sum();
        if sum > 0.0 {
            for (_, p) in distribution.iter_mut() {
                *p /= sum;
            }
        }

        // Apply entanglement from previous message in conversation
        let previous_endpoint = self.last_endpoint.get(&msg.conversation_id).map(|e| e.clone());
        self.entanglement.apply_entanglement(
            msg.conversation_id,
            previous_endpoint.as_deref(),
            &mut distribution,
            0.2,
        );

        // Apply exploration: epsilon-greedy on the quantum distribution
        let mut rng = thread_rng();
        let selected = if rng.gen::<f64>() < self.config.exploration_rate {
            // Explore: uniform random among candidates
            candidates.choose(&mut rng).cloned()
        } else {
            // Exploit: sample from quantum distribution
            let weights: Vec<f64> = distribution.iter().map(|(_, p)| *p).collect();
            if let Ok(dist) = WeightedIndex::new(&weights) {
                let idx = dist.sample(&mut rng);
                Some(distribution[idx].0.clone())
            } else {
                candidates.choose(&mut rng).cloned()
            }
        };

        if let Some(ref ep_id) = selected {
            self.last_endpoint.insert(msg.conversation_id, ep_id.clone());
        }

        selected
    }

    /// Observe the result of routing and update quantum state.
    pub fn observe(&self, conversation_id: uuid::Uuid, endpoint_id: &str, success: bool, latency_ms: u64) {
        // Reward function: 1.0 for success, penalize latency
        let latency_penalty = (latency_ms as f64 / 5000.0).min(1.0);
        let reward = if success {
            1.0 - latency_penalty * 0.3
        } else {
            0.0
        };

        self.state.update(
            conversation_id,
            endpoint_id,
            reward,
            self.config.decoherence_rate,
        );

        // Persist to store if available
        if let Some(ref store) = self.store {
            let ep_state = {
                let state = self.state.get_or_create(conversation_id);
                state.endpoint_states.get(endpoint_id).map(|e| e.clone())
            };
            if let Some(cloned) = ep_state {
                let ep = endpoint_id.to_string();
                let store = store.clone();
                let cid = conversation_id;
                tokio::spawn(async move {
                    let _ = store.save_quantum_state(cid, &ep, &cloned).await;
                });
            }
        }
    }

    /// Manually inject feedback for a routed message.
    pub fn feedback(&self, conversation_id: uuid::Uuid, endpoint_id: &str, quality_score: f32) {
        let reward = (quality_score as f64).clamp(0.0, 1.0);
        self.state.update(
            conversation_id,
            endpoint_id,
            reward,
            self.config.decoherence_rate,
        );
    }

    /// Get diagnostics for a conversation's quantum state.
    pub fn diagnostics(&self, conversation_id: uuid::Uuid) -> Vec<(String, f64, u64, f64)> {
        let conv = self.state.get_or_create(conversation_id);
        conv.endpoint_states
            .iter()
            .map(|e| {
                let s = e.value();
                (
                    e.key().clone(),
                    s.amplitude.probability(),
                    s.pulls,
                    if s.pulls > 0 {
                        s.total_reward / s.pulls as f64
                    } else {
                        0.0
                    },
                )
            })
            .collect()
    }

    /// Periodic maintenance.
    pub fn tick(&self) {
        self.state.prune_old(3600); // Prune conversations older than 1 hour
        self.entanglement.prune_old(self.config.entanglement_window * 2);
    }
}
