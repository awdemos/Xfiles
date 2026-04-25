# SOUL — Agent Personality Guide for Xfiles

> *This project is a research experiment: Plan 9 concepts applied to AI agent communication. Preserve that spirit in every change.*

## How to Approach This Codebase

### 1. Simplicity over Sophistication
Xfiles is inspired by Plan 9: small interfaces, clear namespaces, everything addressable. Do not introduce heavy abstractions, generic frameworks, or enterprise patterns. If a change adds more files than it removes, question it.

### 2. Respect the Experimental Nature
The "quantum" router is a bandit algorithm with physics metaphors. The VFS is in-memory and Plan 9-style. The plumber is rule-based, not a DAG executor. These are intentional design choices. Do not refactor them into "production-grade" systems unless explicitly asked. Preserve the metaphor and the modularity.

### 3. Modular & Replaceable
Every major subsystem (quantum, plumber, VFS, MCP, discovery, store) is a module that can theoretically be swapped. When adding features:
- Put them in their own module or subdirectory.
- Export a clean public interface.
- Wire them into `AppState` and `daemon.rs`, but keep the module decoupled.

### 4. Files and Namespaces Are First-Class
The VFS and namespace modules exist because Plan 9 treats naming as a primary interface. If you add a new resource, consider whether it should be addressable through the VFS or namespaced before defaulting to a new HTTP route.

### 5. Agents Are Peers, Not Clients
The WebSocket agent protocol treats connected agents as autonomous peers in a hub. Do not assume a master/slave relationship. Keep the registry lightweight and avoid orchestration logic.

### 6. Observability Is Not Optional
Every background task, routing decision, and failure should be observable via `tracing`. Metrics and circuit-breaker state should be queryable at runtime. Do not add opaque retry loops or silent failures.

### 7. SQLite Is a Convenience, Not a Requirement
The store is optional. The daemon starts without it. Any feature requiring persistence must degrade gracefully when the store is `None`.

### 8. Test Before Declaring Done
Run `cargo test` and `cargo clippy`. If you add a new module, add at least a smoke test. Integration tests in `tests/` are preferred for end-to-end behavior.

## Tone

- Be concise in code and comments.
- Use the physics/Plan 9 metaphors consistently (plumber, quantum, entanglement, VFS nodes).
- Prefer showing intent through structure rather than explaining it through comments.
