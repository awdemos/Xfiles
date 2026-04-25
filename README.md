# Xfiles

Plan 9-inspired agent communication hub with quantum-mode AI routing.

Xfiles is a lightweight, async Rust daemon that sits between your agents and AI endpoints. It provides virtual filesystem namespaces, probabilistic endpoint selection, auto-discovery, and message routing — all through a unified HTTP/WebSocket API.

## Plan 9 Primitives

[Plan 9](https://en.wikipedia.org/wiki/Plan_9_from_Bell_Labs) was a distributed operating system built on three radical ideas: *everything is a file*, each process gets its own *namespace*, and a central *plumber* routes data between applications based on content rather than hardcoded addresses.

Xfiles adapts these primitives for multi-agent systems:

| Plan 9 Concept | How Xfiles Implements It | Why It Matters for Agents |
|----------------|--------------------------|---------------------------|
| **Everything is a file** | The VFS registry exposes agents, AI endpoints, and system state as paths (`/net/{id}/ctl`, `/ai/ollama`, `/proc/log`). Agents read and write these paths over HTTP just like files. | Agents can discover and interact with any resource through a single, consistent interface. No custom SDKs per service. |
| **Namespaces** | Every agent receives an isolated view of the hub. Its own control file, inbox, and capabilities live under a per-agent prefix. | Agents operate in their own context but remain addressable by others. Security and isolation are properties of naming, not afterthoughts. |
| **Plumber** | The plumber routes messages by inspecting content type, headers, and regex patterns rather than fixed URLs. A message tagged `type:llm_request` automatically flows to an AI endpoint; `type:mcp_tool_call` routes to the right tool provider. | Agents do not need to know where services live. They publish messages; the hub decides delivery based on intent. |
| **Unified addressing** | Agents and services share one namespace. An agent at `/net/alpha` can send a message to `/net/beta` or `/ai/ollama` using the same path syntax. | Location transparency. An agent can address another agent, a model, or a tool with the same addressing scheme, whether local or remote. |

In short: Xfiles treats agents as processes in a distributed OS, giving them the Plan 9 superpower of *naming as the primary interface*.

## Features

- **Plan 9 VFS** — Every agent gets a namespace (`/net/{id}/ctl`, `/msg/inbox`, capabilities)
- **Quantum Router** — Probabilistic endpoint selection with amplitude tracking, exploration/exploitation, and entanglement
- **AI Proxy** — OpenAI-compatible `/v1/chat/completions` with streaming, model aliasing, and transparent forwarding
- **Auto-Discovery** — Port scanning and Docker label-based discovery of AI services
- **Plumber Routing** — Content-based message rules with regex and header matching
- **Circuit Breaker** — Per-endpoint failure detection with automatic recovery
- **Message Queue** — Reliable agent-to-agent delivery with retry and TTL pruning
- **MCP Integration** — Model Context Protocol tool registry and routing
- **Auth & Rate Limiting** — Bearer-token API keys and token-bucket rate limits
- **OpenTelemetry** — Distributed tracing via OTLP export
- **TLS / mTLS** — Optional rustls termination and client certificate verification
- **Persistence** — SQLite storage for messages, quantum state, feedback, and delivery tracking

## Quick Start

```bash
# Build
cargo build --release

# Copy example config
cp xfiles.toml.example xfiles.toml

# Run the daemon
XFILES_API_KEY=secret cargo run -- serve

# Check health
xfiles health

# List agents
xfiles agents --api-key secret

# List endpoints
xfiles endpoints --api-key secret
```

## Architecture

```
┌─────────────┐     WS/HTTP      ┌─────────────────────────────────────┐
│   Agent     │ ◄──────────────► │  Xfiles Hub                         │
│  (any lang) │                  │  ├── VFS Registry (/net, /ai, /proc)│
└─────────────┘                  │  ├── Agent Registry                 │
                                 │  ├── Plumber (content routing)      │
┌─────────────┐     HTTP         │  ├── Quantum Router (probabilistic) │
│  AI Proxy   │ ◄──────────────► │  ├── MCP Registry (tools)           │
│  /v1/chat/  │                  │  ├── Circuit Breaker                │
└─────────────┘                  │  ├── Message Queue                  │
                                 │  └── Store (SQLite)                 │
┌─────────────┐     HTTP         └─────────────────────────────────────┘
│  Ollama/    │ ◄─────────────────────►  Health Probes / Discovery
│  vLLM/etc   │
└─────────────┘
```

## Configuration

See `xfiles.toml.example` for all options. Key sections:

```toml
[hub]
bind_addr = "0.0.0.0:9999"
database_url = "sqlite:xfiles.db"

[auth]
api_key = "your-secret-key"

[rate_limit]
max_requests = 100
window_secs = 60

[circuit]
failure_threshold = 5
recovery_timeout_secs = 30

[tls]
enabled = false
cert_path = "/etc/xfiles/server.crt"
key_path = "/etc/xfiles/server.key"
client_ca_path = "/etc/xfiles/ca.crt"  # optional, for mTLS

[[ai.endpoints]]
name = "ollama"
url = "http://localhost:11434"
endpoint_type = "inference"
```

## WebSocket Protocol

Agents connect to `/ws/{agent_id}` and speak JSON:

```json
{"op": "hello", "manifest": {"agent_id": "my-bot", "hostname": "box1", "capabilities": []}}
{"op": "heartbeat"}
{"op": "message", "id": "...", "conversation_id": "...", "path": "/ai", "type": "llm_request", "data": {"prompt": "hi"}}
{"op": "ack", "message_id": "..."}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `XFILES_CONFIG` | Path to TOML config (default: `xfiles.toml`) |
| `XFILES_API_KEY` | Bearer token for admin/proxy endpoints |
| `XFILES_AGENT_TOKEN` | Bearer token for WebSocket connections |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry collector endpoint (e.g. `http://localhost:4317`) |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/metrics` | Prometheus metrics |
| GET | `/agents` | List connected agents |
| GET | `/endpoints` | List AI endpoints with health |
| GET | `/mcp/tools` | List discovered MCP tools |
| GET | `/fs/*path` | Read VFS file or directory |
| POST | `/fs/*path` | Write VFS file |
| POST | `/msg` | Send a message |
| POST | `/v1/chat/completions` | OpenAI-compatible chat proxy |
| GET | `/v1/models` | OpenAI-compatible models list |
| GET | `/quantum/state` | Quantum diagnostics |
| POST | `/quantum/feedback` | Submit routing feedback |
| GET | `/circuit/state` | Circuit breaker diagnostics |
| GET | `/conversations` | List conversations |
| GET | `/conversations/:id/messages` | Messages in a conversation |
| POST | `/grpc` | gRPC-style msgpack RPC |

## Docker Discovery

Label containers to auto-register:

```yaml
labels:
  ai.xfiles.service: "ollama"
  ai.xfiles.type: "inference"
  ai.xfiles.port: "11434"
```

## License

MIT
