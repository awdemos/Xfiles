use crate::ai::endpoints::{AiEndpoint, HealthStatus};
use crate::circuit::CircuitBreaker;
use crate::message::Message;
use crate::mcp::McpRegistry;
use crate::plumber::Plumber;
use crate::quantum::QuantumRouter;
use std::sync::Arc;

/// Decision produced by the routing pipeline.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub selected: Option<String>,
    pub candidates: Vec<String>,
    pub explanation: String,
}

/// A single stage in the routing pipeline.
pub trait RouterStage: Send + Sync {
    /// Apply this stage to the routing context.
    /// Returns true if routing is complete (final selection made).
    fn apply(&self, msg: &Message, ctx: &mut RoutingContext) -> bool;
}

/// Mutable routing context passed through each stage.
pub struct RoutingContext {
    pub candidates: Vec<String>,
    pub selected: Option<String>,
    pub explanation: Vec<String>,
    pub rejected: Vec<String>,
}

impl RoutingContext {
    pub fn new(candidates: Vec<String>) -> Self {
        Self {
            candidates,
            selected: None,
            explanation: Vec::new(),
            rejected: Vec::new(),
        }
    }

    pub fn into_decision(self) -> RoutingDecision {
        RoutingDecision {
            selected: self.selected,
            candidates: self.candidates,
            explanation: self.explanation.join("; "),
        }
    }
}

/// Composable routing pipeline.
pub struct RoutingPipeline {
    stages: Vec<Box<dyn RouterStage>>,
}

impl std::fmt::Debug for RoutingPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutingPipeline")
            .field("stage_count", &self.stages.len())
            .finish()
    }
}

impl RoutingPipeline {
    pub fn new(stages: Vec<Box<dyn RouterStage>>) -> Self {
        Self { stages }
    }

    pub async fn route(&self, msg: &Message) -> RoutingDecision {
        let mut ctx = RoutingContext::new(Vec::new());

        for stage in &self.stages {
            let is_final = stage.apply(msg, &mut ctx);
            if is_final {
                break;
            }
        }

        ctx.into_decision()
    }
}

// ------------------------------------------------------------------
// Stage implementations
// ------------------------------------------------------------------

/// Detects MCP tool calls and routes to the appropriate tool endpoint.
pub struct McpRoutingStage {
    mcp: Arc<McpRegistry>,
}

impl McpRoutingStage {
    pub fn new(mcp: Arc<McpRegistry>) -> Self {
        Self { mcp }
    }
}

impl RouterStage for McpRoutingStage {
    fn apply(&self, msg: &Message, ctx: &mut RoutingContext) -> bool {
        if msg.msg_type == "mcp_tool_call" {
            let tool_name = msg
                .data
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if let Some(endpoint) = self.mcp.find_endpoint_for_tool(tool_name) {
                ctx.selected = Some(endpoint.clone());
                ctx.explanation.push(format!("mcp tool '{}' → {}", tool_name, endpoint));
                return true;
            }
        }
        false
    }
}

/// Routes based on plumber content rules.
pub struct PlumberRoutingStage {
    plumber: Plumber,
}

impl PlumberRoutingStage {
    pub fn new(plumber: Plumber) -> Self {
        Self { plumber }
    }
}

impl RouterStage for PlumberRoutingStage {
    fn apply(&self, msg: &Message, ctx: &mut RoutingContext) -> bool {
        let destinations = self.plumber.route(msg);
        ctx.explanation.push(format!("plumber matched {} rule(s)", destinations.len()));
        ctx.candidates = destinations;
        false // Never final - always passes candidates to next stage
    }
}

/// Probabilistic endpoint selection using the quantum router.
pub struct QuantumRoutingStage {
    quantum: Arc<QuantumRouter>,
}

impl QuantumRoutingStage {
    pub fn new(quantum: Arc<QuantumRouter>) -> Self {
        Self { quantum }
    }
}

impl RouterStage for QuantumRoutingStage {
    fn apply(&self, msg: &Message, ctx: &mut RoutingContext) -> bool {
        if ctx.candidates.is_empty() {
            return false;
        }

        // Filter out rejected candidates
        let valid_candidates: Vec<String> = ctx
            .candidates
            .iter()
            .filter(|c| !ctx.rejected.contains(c))
            .cloned()
            .collect();

        if valid_candidates.is_empty() {
            ctx.explanation.push("quantum: no valid candidates".into());
            return false;
        }

        match self.quantum.route(msg, &valid_candidates) {
            Some(selected) => {
                ctx.explanation.push(format!("quantum selected {}", selected));
                ctx.selected = Some(selected);
                true
            }
            None => {
                ctx.explanation.push("quantum: no selection".into());
                false
            }
        }
    }
}

/// Fallback stage that selects the first available candidate.
pub struct FallbackRoutingStage;

impl RouterStage for FallbackRoutingStage {
    fn apply(&self, _msg: &Message, ctx: &mut RoutingContext) -> bool {
        let valid_candidates: Vec<String> = ctx
            .candidates
            .iter()
            .filter(|c| !ctx.rejected.contains(c))
            .cloned()
            .collect();

        if let Some(first) = valid_candidates.first() {
            ctx.explanation.push(format!("fallback selected {}", first));
            ctx.selected = Some(first.clone());
            true
        } else {
            ctx.explanation.push("fallback: no candidates available".into());
            false
        }
    }
}

/// Filters candidates through the circuit breaker.
pub struct CircuitAwareStage {
    circuit: Arc<CircuitBreaker>,
    endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>,
}

impl CircuitAwareStage {
    pub fn new(circuit: Arc<CircuitBreaker>, endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>) -> Self {
        Self { circuit, endpoints }
    }
}

impl RouterStage for CircuitAwareStage {
    fn apply(&self, _msg: &Message, ctx: &mut RoutingContext) -> bool {
        let before_count = ctx.candidates.len();

        ctx.candidates.retain(|ep_id| {
            // Check circuit breaker
            if !self.circuit.allow(ep_id) {
                ctx.rejected.push(ep_id.clone());
                return false;
            }

            // Check endpoint health
            if let Some(ep) = self.endpoints.get(ep_id) {
                if ep.health.status == HealthStatus::Offline {
                    ctx.rejected.push(ep_id.clone());
                    return false;
                }
            }

            true
        });

        let after_count = ctx.candidates.len();
        if after_count < before_count {
            ctx.explanation.push(format!(
                "circuit breaker filtered {} endpoint(s)",
                before_count - after_count
            ));
        }

        false // Never final - just filters candidates
    }
}

/// Builds the default routing pipeline.
pub fn default_pipeline(
    mcp: Arc<McpRegistry>,
    plumber: Plumber,
    quantum: Option<Arc<QuantumRouter>>,
    circuit: Option<Arc<CircuitBreaker>>,
    endpoints: Arc<dashmap::DashMap<String, AiEndpoint>>,
) -> RoutingPipeline {
    let mut stages: Vec<Box<dyn RouterStage>> = Vec::new();

    stages.push(Box::new(McpRoutingStage::new(mcp)));
    stages.push(Box::new(PlumberRoutingStage::new(plumber)));

    if let Some(q) = quantum {
        stages.push(Box::new(QuantumRoutingStage::new(q)));
    }

    if let Some(c) = circuit {
        stages.push(Box::new(CircuitAwareStage::new(c, endpoints)));
    }

    stages.push(Box::new(FallbackRoutingStage));

    RoutingPipeline::new(stages)
}
