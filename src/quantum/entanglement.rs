use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Tracks correlations between endpoints within a conversation.
#[derive(Debug, Clone, Default)]
pub struct EntanglementTable {
    /// conversation_id -> (endpoint_a, endpoint_b) -> correlation score [-1, 1]
    correlations: Arc<DashMap<Uuid, DashMap<(String, String), f64>>>,
}

impl EntanglementTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that endpoint_a and endpoint_b were both used successfully in the same conversation.
    pub fn record_pair(&self, conversation_id: Uuid, endpoint_a: &str, endpoint_b: &str, reward: f64) {
        let conv_map = self
            .correlations
            .entry(conversation_id)
            .or_insert_with(DashMap::new);

        let key = Self::ordered_key(endpoint_a, endpoint_b);
        let mut entry = conv_map.entry(key).or_insert(0.0);
        // Exponential moving average of correlation
        *entry = *entry * 0.9 + reward * 0.1;
    }

    /// Get the correlation between two endpoints in a conversation.
    pub fn get_correlation(&self, conversation_id: Uuid, endpoint_a: &str, endpoint_b: &str) -> f64 {
        self.correlations
            .get(&conversation_id)
            .and_then(|conv| {
                let key = Self::ordered_key(endpoint_a, endpoint_b);
                conv.get(&key).map(|e| *e)
            })
            .unwrap_or(0.0)
    }

    /// Apply entanglement boosts to a probability distribution.
    pub fn apply_entanglement(
        &self,
        conversation_id: Uuid,
        previous_endpoint: Option<&str>,
        distribution: &mut [(String, f64)],
        strength: f64,
    ) {
        let Some(prev) = previous_endpoint else { return };
        for (ep, prob) in distribution.iter_mut() {
            let corr = self.get_correlation(conversation_id, prev, ep);
            // Boost or suppress based on correlation
            *prob *= 1.0 + corr * strength;
        }
        // Re-normalize
        let sum: f64 = distribution.iter().map(|(_, p)| p).sum();
        if sum > 0.0 {
            for (_, p) in distribution.iter_mut() {
                *p /= sum;
            }
        }
    }

    fn ordered_key(a: &str, b: &str) -> (String, String) {
        if a < b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        }
    }

    pub fn prune_old(&self, max_age_conversations: usize) {
        if self.correlations.len() > max_age_conversations {
            let to_remove: Vec<Uuid> = self
                .correlations
                .iter()
                .take(self.correlations.len() - max_age_conversations)
                .map(|e| *e.key())
                .collect();
            for id in to_remove {
                self.correlations.remove(&id);
            }
        }
    }
}
