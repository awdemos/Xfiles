use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

/// A complex probability amplitude: a + bi.
#[derive(Debug, Clone, Copy, Default)]
pub struct Amplitude {
    pub real: f64,
    pub imag: f64,
}

impl Amplitude {
    pub fn new(real: f64, imag: f64) -> Self {
        Self { real, imag }
    }

    /// Probability = |amplitude|^2.
    pub fn probability(&self) -> f64 {
        self.real * self.real + self.imag * self.imag
    }

    pub fn magnitude(&self) -> f64 {
        self.probability().sqrt()
    }

    pub fn phase(&self) -> f64 {
        self.imag.atan2(self.real)
    }

    /// Normalize so probability sums to 1 across a slice.
    pub fn normalize_all(amplitudes: &mut [Amplitude]) {
        let sum: f64 = amplitudes.iter().map(|a| a.probability()).sum();
        if sum > 0.0 {
            let scale = 1.0 / sum.sqrt();
            for a in amplitudes.iter_mut() {
                a.real *= scale;
                a.imag *= scale;
            }
        }
    }
}

/// Per-endpoint quantum state tracked by the router.
#[derive(Debug, Clone)]
pub struct EndpointState {
    pub amplitude: Amplitude,
    pub pulls: u64,
    pub total_reward: f64,
    pub last_updated: DateTime<Utc>,
}

impl Default for EndpointState {
    fn default() -> Self {
        Self {
            amplitude: Amplitude::new(1.0, 0.0),
            pulls: 0,
            total_reward: 0.0,
            last_updated: Utc::now(),
        }
    }
}

/// Quantum state for a single conversation (entanglement group).
#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    pub conversation_id: Uuid,
    pub endpoint_states: Arc<DashMap<String, EndpointState>>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

impl ConversationState {
    pub fn new(conversation_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            conversation_id,
            endpoint_states: Arc::new(DashMap::new()),
            created_at: now,
            last_active: now,
        }
    }

    pub fn get_or_insert(&self, endpoint_id: &str) -> EndpointState {
        self.endpoint_states
            .entry(endpoint_id.to_string())
            .or_insert_with(EndpointState::default)
            .clone()
    }

    pub fn update(&self, endpoint_id: &str, reward: f64, decoherence_rate: f64) {
        let now = Utc::now();
        {
            let mut entry = self
                .endpoint_states
                .entry(endpoint_id.to_string())
                .or_insert_with(EndpointState::default);

            entry.pulls += 1;
            entry.total_reward += reward;
            entry.last_updated = now;

            // Update amplitude based on empirical reward
            let avg_reward = entry.total_reward / entry.pulls as f64;
            let target_mag = avg_reward.sqrt().clamp(0.0, 1.0);

            // Rotate phase slightly based on reward (interference)
            let phase_shift = (reward - 0.5) * std::f64::consts::PI;
            let current_phase = entry.amplitude.phase();
            let new_phase = current_phase + phase_shift * 0.1;

            entry.amplitude.real = target_mag * new_phase.cos();
            entry.amplitude.imag = target_mag * new_phase.sin();
        } // entry dropped here, releasing the shard lock

        // Apply decoherence: amplitudes decay toward uniform
        self.apply_decoherence(decoherence_rate);
    }

    fn apply_decoherence(&self, rate: f64) {
        let entries: Vec<_> = self.endpoint_states.iter().map(|e| e.key().clone()).collect();
        if entries.is_empty() {
            return;
        }
        let uniform_mag = 1.0 / (entries.len() as f64).sqrt();
        for key in entries {
            if let Some(mut entry) = self.endpoint_states.get_mut(&key) {
                entry.amplitude.real = entry.amplitude.real * (1.0 - rate) + uniform_mag * rate;
                entry.amplitude.imag = entry.amplitude.imag * (1.0 - rate);
            }
        }
    }

    pub fn compute_distribution(&self) -> Vec<(String, f64)> {
        let mut dist: Vec<(String, f64)> = self
            .endpoint_states
            .iter()
            .map(|e| (e.key().clone(), e.amplitude.probability()))
            .collect();

        let total: f64 = dist.iter().map(|(_, p)| p).sum();
        if total > 0.0 {
            for (_, p) in dist.iter_mut() {
                *p /= total;
            }
        }
        dist
    }
}

/// Global quantum state manager.
#[derive(Debug, Clone, Default)]
pub struct QuantumStateManager {
    conversations: Arc<DashMap<Uuid, ConversationState>>,
}

impl QuantumStateManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&self, conversation_id: Uuid) -> ConversationState {
        self.conversations
            .entry(conversation_id)
            .or_insert_with(|| ConversationState::new(conversation_id))
            .clone()
    }

    pub fn get(&self, conversation_id: Uuid) -> Option<ConversationState> {
        self.conversations.get(&conversation_id).map(|e| e.clone())
    }

    pub fn update(
        &self,
        conversation_id: Uuid,
        endpoint_id: &str,
        reward: f64,
        decoherence_rate: f64,
    ) {
        let conv = self.get_or_create(conversation_id);
        conv.update(endpoint_id, reward, decoherence_rate);
    }

    pub fn prune_old(&self, max_age_secs: i64) {
        let now = Utc::now();
        let to_remove: Vec<Uuid> = self
            .conversations
            .iter()
            .filter(|e| (now - e.last_active).num_seconds() > max_age_secs)
            .map(|e| *e.key())
            .collect();
        for id in to_remove {
            self.conversations.remove(&id);
        }
    }

    pub fn conversation_count(&self) -> usize {
        self.conversations.len()
    }

    pub fn all_diagnostics(&self) -> Vec<(Uuid, Vec<(String, f64, u64, f64)>)> {
        self.conversations
            .iter()
            .map(|e| {
                let conv = e.value();
                let diag: Vec<(String, f64, u64, f64)> = conv
                    .endpoint_states
                    .iter()
                    .map(|ep| {
                        let s = ep.value();
                        (
                            ep.key().clone(),
                            s.amplitude.probability(),
                            s.pulls,
                            if s.pulls > 0 {
                                s.total_reward / s.pulls as f64
                            } else {
                                0.0
                            },
                        )
                    })
                    .collect();
                (*e.key(), diag)
            })
            .collect()
    }
}
