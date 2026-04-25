# Agent Guide for Xfiles

This file describes the project state and practical workflows for AI coding assistants working on Xfiles.

## Build

```bash
cargo build          # debug build
cargo build --release
```

The project uses the **2021 edition** and targets **Rust 1.82+**.

Key crates: `axum`, `tokio`, `sqlx` (SQLite), `reqwest`, `serde`, `clap`, `prometheus`, `opentelemetry`, `rustls`.

## Run

```bash
# Start the daemon
cargo run -- serve

# Or the binary directly
./target/debug/xfiles serve
./target/release/xfiles serve
```

The daemon reads configuration from `xfiles.toml` by default (override with `XFILES_CONFIG`).

## Test

```bash
cargo test
```

Integration tests live in `tests/`:
- `integration_test.rs`
- `plumber_test.rs`
- `quantum_test.rs`
- `vfs_test.rs`

## Lint / Format

```bash
cargo fmt
cargo clippy
```

## Configuration

Copy `xfiles.toml.example` to `xfiles.toml` and customize. The example is also embedded in the binary and printable via:

```bash
cargo run -- config
```

Key runtime env vars:
- `XFILES_CONFIG` — path to config file
- `XFILES_BIND` — override bind address
- `XFILES_MAX_AGENTS` — override agent limit
- `XFILES_QUANTUM_ENABLED` — override quantum toggle
- `RUST_LOG` — tracing filter (e.g. `xfiles=debug,tower_http=debug`)

## Architecture at a Glance

- **Entry**: `src/main.rs` (CLI with `clap`), `src/daemon.rs` (server lifecycle)
- **HTTP router**: `axum` in `src/daemon.rs` — routes are defined explicitly near the top of `run()`
- **State**: `AppState` holds shared registries (agents, endpoints, VFS, plumber, quantum, queue, MCP, store)
- **Config**: `src/config.rs` — TOML + env overrides. Defaults are provided for all fields.
- **AI layer**: `src/ai/` — endpoint definitions, health probes, port discovery, Docker discovery
- **Routing**: `src/plumber.rs` (rule-based) + `src/quantum/` (bandit-based per-conversation)
- **Proxy**: `src/proxy.rs` — OpenAI-compatible `/v1/chat/completions` and `/v1/models`
- **Persistence**: `src/store.rs` — SQLite via `sqlx`, optional (runs without it if DB init fails)
- **Agents**: `src/agent.rs` — WebSocket-connected agents with heartbeats
- **VFS**: `src/fs/` — in-memory Plan 9-style file system exposed over HTTP
- **MCP**: `src/mcp.rs` — Model Context Protocol tool discovery across endpoints
- **Resilience**: `src/circuit.rs` (circuit breaker), `src/ratelimit.rs` (token bucket)
- **Observability**: `src/telemetry.rs` (OTel + tracing), Prometheus metrics exposed at `/metrics`
- **TLS**: `src/tls.rs` — optional TLS and mTLS support via `rustls`

## Database Migrations

SQLx migration files are in `migrations/`:
- `001_init.sql`
- `002_delivery_tracking.sql`

The store initializes these automatically on startup if SQLite is available.

## Docker

A multi-stage `Dockerfile` is provided. Build and run with:

```bash
docker compose up --build
```

The compose file mounts `./xfiles.toml` into the container.

## Adding a New HTTP Route

1. Add the route in `src/daemon.rs` inside the `Router::new()` chain.
2. Implement the handler nearby (or in the relevant module).
3. Ensure `AppState` has whatever shared state the handler needs.
4. If the route should be auth-protected, leave it inside the main router — auth middleware is applied globally after route construction.
5. If the route should bypass rate limiting, note that rate limiting is also applied as a global layer after auth.

## Adding a New Background Task

Background tasks are spawned in `src/daemon.rs::run()` before the server starts. Follow the existing pattern:
- Create a shutdown channel (`tokio::sync::watch::channel(false)`)
- Spawn the task
- Add graceful shutdown cleanup near the bottom of `run()`

## Code Style

- Use `anyhow::Result` for application errors and `thiserror` for library error enums.
- Prefer `tracing` (not `println`) for all logging.
- Async throughout — all handlers and store methods are `async`.
- Use `Arc<DashMap<...>>` for shared concurrent state.
- Modules are public (`pub mod`) in `src/lib.rs` so they can be tested externally.
