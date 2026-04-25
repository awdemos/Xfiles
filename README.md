# Xfiles

**Plan 9-inspired agent communication hub with quantum-mode AI routing.**

Xfiles is a Rust-based infrastructure node that sits between your agents and AI services. It provides a unified routing layer, dynamic endpoint discovery, resilient proxying, and an optional "quantum" multi-armed bandit router that learns which AI endpoint works best per conversation.

---

## What It Does

| Feature | Description |
|---------|-------------|
| **AI Proxy** | Drop-in `/v1/chat/completions` and `/v1/models` proxy compatible with OpenAI-style clients. |
| **Quantum Router** | Conversation-aware bandit routing that explores and exploits AI endpoints based on observed latency and success. |
| **Agent Registry** | WebSocket-based agent connections with heartbeats, namespaces, and stale-agent pruning. |
| **Auto Discovery** | Scans ports and optionally polls Docker for new AI endpoints to add dynamically. |
| **MCP Tools** | Discovers and indexes Model Context Protocol tools across registered endpoints. |
| **Plumber Rules** | Declarative message routing with pattern matching and header filters. |
| **VFS** | Plan 9-style in-memory virtual file system accessible over HTTP. |
| **Circuit Breaker** | Automatic failure detection and recovery for downstream AI endpoints. |
| **Rate Limiting** | Token-bucket rate limiting on all HTTP routes. |
| **Observability** | Prometheus metrics, OpenTelemetry tracing, and structured logging out of the box. |
| **Persistence** | SQLite storage for messages, feedback, and conversation history. |

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.82+ recommended)
- (Optional) Docker & Docker Compose

### Build

```bash
git clone https://github.com/awdemos/Xfiles.git
cd Xfiles
cargo build --release
```

### Configure

Copy the example configuration and edit it to point at your AI endpoints:

```bash
cp xfiles.toml.example xfiles.toml
```

Key sections to customize:

- `[[ai.endpoints]]` — Add your LLM routers, inference servers, or memory stores.
- `[auth]` — Uncomment and set `api_key` / `agent_token` if you want Bearer-token auth.
- `[quantum]` — Tune exploration vs. exploitation rates (or disable with `enabled = false`).

### Run

```bash
# Using the CLI
cargo run -- serve

# Or the release binary
./target/release/xfiles serve
```

By default the hub listens on `0.0.0.0:9999`.

---

## CLI

```
xfiles serve                  # Start the daemon
xfiles config                 # Print example configuration (TOML)
xfiles health                 # GET /health from a running instance
xfiles agents                 # List connected agents
xfiles endpoints              # List AI endpoints and their health
xfiles metrics                # Fetch Prometheus metrics
```

---

## Docker

```bash
docker compose up --build
```

The included `docker-compose.yml` mounts `./xfiles.toml` into the container at `/etc/xfiles/xfiles.toml`.

---

## API Overview

| Route | Method | Purpose |
|-------|--------|---------|
| `/health` | `GET` | Liveness probe |
| `/metrics` | `GET` | Prometheus metrics |
| `/v1/chat/completions` | `POST` | OpenAI-compatible chat proxy |
| `/v1/models` | `GET` | OpenAI-compatible model list |
| `/agents` | `GET` | List connected WebSocket agents |
| `/endpoints` | `GET` | List registered AI endpoints with health |
| `/mcp/tools` | `GET` | Discover MCP tools across endpoints |
| `/msg` | `POST` | Send a routed message through the plumber |
| `/fs/*path` | `GET/POST` | Read/write virtual file system nodes |
| `/quantum/state` | `GET` | Inspect quantum router diagnostics |
| `/quantum/feedback` | `POST` | Submit reward feedback for a routing decision |
| `/circuit/state` | `GET` | Inspect circuit breaker states |
| `/conversations` | `GET` | List persisted conversations |
| `/conversations/:id/messages` | `GET` | Messages for a conversation |
| `/ws/:agent_id` | `GET` | WebSocket agent connection endpoint |

---

## Configuration Reference

Xfiles loads `xfiles.toml` by default (override with `XFILES_CONFIG` env var).

```toml
[hub]
bind_addr = "0.0.0.0:9999"
max_agents = 100
heartbeat_interval_secs = 30
probe_interval_secs = 30
database_url = "sqlite:xfiles.db"

default_model = "kimi-k2.6"

[model_aliases]
default = "kimi-k2.6"
gpt-4o = "kimi-k2.6"

[auth]
api_key = "super-secret-key"      # Protects proxy & admin endpoints
agent_token = "agent-secret"       # Protects WebSocket connections

[rate_limit]
enabled = true
max_requests = 100
window_secs = 60

[quantum]
enabled = true
decoherence_rate = 0.05
exploration_rate = 0.15
entanglement_window = 10
min_samples_before_exploit = 5

[discovery]
scan_ports = [3000, 8080, 7777, 8000, 11434, 9000]
scan_interval_secs = 60
docker_enabled = false

[[ai.endpoints]]
name = "my-llm"
url = "http://localhost:8080"
endpoint_type = "router"
weight = 1.0
tags = ["local"]

[[plumber.rules]]
name = "route_to_ai"
pattern = "type:llm_request"
destination = "/ai/route"
priority = 100
```

### Environment Overrides

| Variable | Effect |
|----------|--------|
| `XFILES_CONFIG` | Path to config file |
| `XFILES_BIND` | Override bind address |
| `XFILES_MAX_AGENTS` | Override agent limit |
| `XFILES_QUANTUM_ENABLED` | Override quantum toggle |

---

## Project Structure

```
src/
  main.rs          # CLI entrypoint
  lib.rs           # Public module exports
  daemon.rs        # HTTP server, background tasks, graceful shutdown
  config.rs        # TOML/env configuration types
  agent.rs         # Agent registry and WebSocket lifecycle
  ai/              # Endpoint definitions, health probes, discovery
  auth.rs          # Bearer-token middleware
  circuit.rs       # Circuit breaker for downstream endpoints
  docker.rs        # Docker-based endpoint discovery
  fs/              # Virtual file system (Plan 9-style)
  grpc.rs          # MessagePack-flavored gRPC codec
  mcp.rs           # Model Context Protocol tool registry
  message.rs       # Message types and feedback events
  namespace.rs     # Agent namespaces
  net/             # Transport layer and protocol ops
  plumber.rs       # Rule-based message router
  proxy.rs         # OpenAI-compatible proxy handlers
  quantum/         # Bandit-based quantum router
  queue.rs         # Offline agent message queue
  ratelimit.rs     # Token-bucket rate limiter
  store.rs         # SQLite persistence layer
  telemetry.rs     # OpenTelemetry + tracing setup
  tls.rs           # TLS/mTLS configuration
```

---

## License

MIT
