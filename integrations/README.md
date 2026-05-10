# AWDemos Platform Integration

This directory contains the integration layer that connects the awdemos projects into a cohesive developer platform.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Developer Interface                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ openlibertas │  │ opencode    │  │ kairo-cli           │ │
│  │   (TUI)      │  │   (Editor)  │  │   (CLI/Scripts)     │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
└─────────┼────────────────┼────────────────────┼────────────┘
          │                │                    │
          │ HTTP           │ Plugin API         │ HTTP/WS
          │                │                    │
┌─────────▼────────────────▼────────────────────▼─────────────┐
│                      Xfiles Hub                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ VFS Registry│  │ Quantum     │  │ MCP Registry        │ │
│  │ (/net, /ai) │  │ Router      │  │ (Tool Discovery)    │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ Plumber     │  │ Circuit     │  │ Message Queue       │ │
│  │ (Routing)   │  │ Breaker     │  │ (Agent→Agent)       │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────┬────────────────┬────────────────────┬────────────┘
          │                │                    │
          │ OpenAI API     │ HTTP API           │ HTTP API
          │                │                    │
┌─────────▼────────────────▼────────────────────▼─────────────┐
│                    Backend Services                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ kairo-api   │  │ llama.cpp   │  │ Ollama / vLLM       │ │
│  │ (Agents)    │  │ (Local LLM) │  │ (Local LLM)         │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Projects

| Project | Role | Interface |
|---------|------|-----------|
| **Xfiles** | Communication hub, message routing, AI proxy | HTTP/WebSocket API on port 9999 |
| **openlibertas** | Terminal UI for chat and agents | TUI app, connects to Xfiles as backend |
| **kairo** | Agent orchestration, workflow engine | HTTP API, registers with Xfiles as agent |
| **opencode-memento** | Context preservation across sessions | OpenCode plugin |

## Quick Start

### 1. Start the Hub

```bash
cd integrations
docker compose -f docker-compose.platform.yml up -d
```

This starts:
- Xfiles hub on port 9999
- SQLite database for persistence
- Pre-configured with local AI endpoints

### 2. Configure openlibertas

Copy the example config:

```bash
cp integrations/configs/openlibertas.toml ~/.config/openlibertas/config.toml
```

This points openlibertas at Xfiles instead of directly at AI endpoints:

```toml
[[providers]]
name = "xfiles"
base_url = "http://localhost:9999/v1"
api_key = "xfiles-secret"
enabled = true
supports_tools = true
```

Now openlibertas gets:
- Quantum routing across all registered endpoints
- Circuit breaker protection
- MCP tool discovery via Xfiles
- Agent-to-agent messaging

### 3. Configure kairo

Start kairo API server and register it with Xfiles:

```bash
# Terminal 1: Start kairo API
cd ../kairo
export OPENAI_API_KEY="your-key"
./target/release/kairo-cli server --port 3000

# Terminal 2: Register kairo as an Xfiles endpoint
./target/release/xfiles endpoints add \
  --name kairo \
  --url http://localhost:3000 \
  --type agent-orchestrator
```

### 4. Verify Integration

```bash
cd integrations
./check.sh
```

## How It Works

### Xfiles as the Backbone

Xfiles provides a unified namespace for all components:

- **AI endpoints** registered at `/ai/{name}` (ollama, llamacpp, kairo)
- **Agents** registered at `/net/{id}` (openlibertas users, kairo agents)
- **Tools** discovered via MCP at `/mcp/tools`
- **State** exposed as VFS files at `/fs/...`

### Quantum Routing

When openlibertas sends a chat request through Xfiles:

1. Request arrives at `/v1/chat/completions`
2. Xfiles' quantum router scores available endpoints
3. Best endpoint selected based on latency, success rate, capability
4. Circuit breaker prevents routing to failing endpoints
5. Response streamed back to openlibertas

### Agent Orchestration

When kairo is registered with Xfiles:

1. Kairo's workflow engine exposes tasks via its HTTP API
2. Xfiles routes agent-to-agent messages through its queue
3. openlibertas can trigger kairo workflows via the hub
4. Kairo agents can send messages back to openlibertas users

## Configuration Files

- `configs/openlibertas.toml` — Points TUI at Xfiles proxy
- `configs/xfiles.toml` — Hub config with all endpoints pre-registered
- `configs/kairo.env` — Environment vars for kairo API server

## Extending

To add a new project to the integration:

1. Expose an OpenAI-compatible `/v1/chat/completions` endpoint, OR
2. Connect to Xfiles' WebSocket API at `/ws/{agent_id}`, OR
3. Register as an HTTP endpoint in Xfiles config

See [Xfiles README](../README.md) for the full API specification.
