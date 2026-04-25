use crate::message::Message;
use crate::net::protocol::ProtocolOp;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing;

/// A queued message waiting for an agent to come back online.
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub msg: Message,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub retries: u32,
}

/// Per-agent in-memory message queue with TTL.
#[derive(Debug, Clone, Default)]
pub struct MessageQueue {
    queues: Arc<DashMap<String, VecDeque<QueuedMessage>>>,
}

impl MessageQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a message for an agent.
    pub fn enqueue(&self, agent_id: &str, msg: Message) {
        let qm = QueuedMessage {
            msg,
            enqueued_at: chrono::Utc::now(),
            retries: 0,
        };
        self.queues
            .entry(agent_id.to_string())
            .or_insert_with(VecDeque::new)
            .push_back(qm);
        tracing::info!("queued message for agent {}", agent_id);
    }

    /// Attempt to deliver all queued messages for an agent.
    pub fn drain_to(&self, agent_id: &str, tx: &mpsc::UnboundedSender<ProtocolOp>) -> usize {
        let mut delivered = 0;
        if let Some(mut entry) = self.queues.get_mut(agent_id) {
            while let Some(qm) = entry.pop_front() {
                if let Err(e) = tx.send(ProtocolOp::Message { msg: qm.msg.clone() }) {
                    tracing::warn!("failed to deliver queued message: {}", e);
                    // Re-queue at front
                    entry.push_front(qm);
                    break;
                }
                delivered += 1;
            }
            if entry.is_empty() {
                drop(entry);
                self.queues.remove(agent_id);
            }
        }
        if delivered > 0 {
            tracing::info!("delivered {} queued messages to agent {}", delivered, agent_id);
        }
        delivered
    }

    /// Prune messages older than max_age_secs.
    pub fn prune_old(&self, max_age_secs: i64) {
        let now = chrono::Utc::now();
        for mut entry in self.queues.iter_mut() {
            let before = entry.len();
            entry.retain(|qm| (now - qm.enqueued_at).num_seconds() < max_age_secs);
            let after = entry.len();
            if after < before {
                tracing::info!("pruned {} stale queued messages", before - after);
            }
        }
        // Remove empty queues
        let empty: Vec<String> = self
            .queues
            .iter()
            .filter(|e| e.value().is_empty())
            .map(|e| e.key().clone())
            .collect();
        for id in empty {
            self.queues.remove(&id);
        }
    }

    /// Stats for diagnostics.
    pub fn stats(&self) -> Vec<(String, usize)> {
        self.queues
            .iter()
            .map(|e| (e.key().clone(), e.value().len()))
            .collect()
    }
}
