use crate::agent::AgentRegistry;
use crate::ai::endpoints::AiEndpoint;
use crate::circuit::CircuitBreaker;
use crate::config::Config;
use crate::event::{EventEmitter, Event};
use crate::fs::VfsRegistry;
use crate::mcp::McpRegistry;
use crate::plumber::Plumber;
use crate::quantum::QuantumRouter;
use crate::queue::MessageQueue;
use crate::store::Store;
use dashmap::DashMap;
use std::sync::Arc;

/// Centralized state manager that owns all shared application state.
///
/// Replaces the scattered God State problem (AppState, TransportState,
/// ApiState, ProxyState) with a single source of truth.
#[derive(Debug, Clone)]
pub struct StateManager {
    inner: Arc<StateManagerInner>,
}

struct StateManagerInner {
    agents: AgentRegistry,
    endpoints: Arc<DashMap<String, AiEndpoint>>,
    vfs: VfsRegistry,
    plumber: Plumber,
    quantum: Option<Arc<QuantumRouter>>,
    queue: Arc<MessageQueue>,
    store: Arc<Store>,
    mcp: Arc<McpRegistry>,
    circuit: Option<Arc<CircuitBreaker>>,
    config: Arc<Config>,
    emitter: Arc<dyn EventEmitter>,
}

impl std::fmt::Debug for StateManagerInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManagerInner")
            .field("agents", &self.agents)
            .field("endpoints", &self.endpoints)
            .field("vfs", &self.vfs)
            .field("plumber", &self.plumber)
            .field("quantum", &self.quantum)
            .field("queue", &self.queue)
            .field("store", &self.store)
            .field("mcp", &self.mcp)
            .field("circuit", &self.circuit)
            .field("config", &self.config)
            .field("emitter", &"<dyn EventEmitter>")
            .finish()
    }
}

impl StateManager {
    pub fn new(
        agents: AgentRegistry,
        endpoints: Arc<DashMap<String, AiEndpoint>>,
        vfs: VfsRegistry,
        plumber: Plumber,
        quantum: Option<Arc<QuantumRouter>>,
        queue: Arc<MessageQueue>,
        store: Arc<Store>,
        mcp: Arc<McpRegistry>,
        circuit: Option<Arc<CircuitBreaker>>,
        config: Arc<Config>,
        emitter: Arc<dyn EventEmitter>,
    ) -> Self {
        Self {
            inner: Arc::new(StateManagerInner {
                agents,
                endpoints,
                vfs,
                plumber,
                quantum,
                queue,
                store,
                mcp,
                circuit,
                config,
                emitter,
            }),
        }
    }

    // Typed accessors - no raw DashMap access outside this module
    pub fn agents(&self) -> &AgentRegistry {
        &self.inner.agents
    }

    pub fn endpoints(&self) -> &Arc<DashMap<String, AiEndpoint>> {
        &self.inner.endpoints
    }

    pub fn vfs(&self) -> &VfsRegistry {
        &self.inner.vfs
    }

    pub fn plumber(&self) -> &Plumber {
        &self.inner.plumber
    }

    pub fn quantum(&self) -> Option<&Arc<QuantumRouter>> {
        self.inner.quantum.as_ref()
    }

    pub fn queue(&self) -> &Arc<MessageQueue> {
        &self.inner.queue
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.inner.store
    }

    pub fn mcp(&self) -> &Arc<McpRegistry> {
        &self.inner.mcp
    }

    pub fn circuit(&self) -> Option<&Arc<CircuitBreaker>> {
        self.inner.circuit.as_ref()
    }

    pub fn config(&self) -> &Arc<Config> {
        &self.inner.config
    }

    pub fn emit(&self, event: Event) {
        self.inner.emitter.emit(event);
    }
}
