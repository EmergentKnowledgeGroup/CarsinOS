# AppDex Executable Ticket Backlog: carsinOS Connector Hub + Connector Apps

Date: 2026-02-27  
Owner: AppDex  
Scope: Add remote connector support (Windows/Android clients) with safe, auditable capability execution.

Reference: `APPDEX_RMP_CONNECTORS_SPEC.md`

## P0 (Foundation: Hub + One Working Connector)

### CONN-001 Connector Runtime Config + Secret Refs
Priority: P0  
Files: `crates/carsinos-protocol/src/lib.rs`, `crates/carsinos-gateway/src/main.rs`  
Deliver:
1. Add runtime config section for connectors:
   - `connectors.enabled` (bool)
   - `connectors.items[]` (id, label, platform_hint, token_secret_ref, allowlisted_capabilities[], allowlisted_tools[], enabled)
   - global guardrails: max in-flight calls per connector, max payload sizes, heartbeat interval expectations
2. Ensure secrets are stored via existing runtime secret endpoints; config stores only secret refs.
Acceptance:
1. Runtime config GET/POST round-trips connector config.
2. Disabling connector (or global connectors) immediately blocks connector actions.

### CONN-002 Connector Hub Persistence (DB)
Priority: P0  
Files: `crates/carsinos-storage/src/lib.rs`, `migrations/`, `crates/carsinos-gateway/src/main.rs`  
Deliver:
1. Tables for:
   - connectors (static metadata + enabled flag)
   - connector_sessions (connected_at, last_seen_at, last_error)
   - connector_calls (call_id, connector_id, tool_name, status, timings, audit refs)
2. Minimal query helpers for list/get + update last_seen.
Acceptance:
1. Connector status survives gateway restart.
2. Call history is queryable by connector id.

### CONN-003 Connector Protocol v1 (Gateway Side)
Priority: P0  
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`  
Deliver:
1. Define a minimal message schema (JSON) for:
   - `connector.hello`, `connector.welcome`
   - `connector.heartbeat`
   - `connector.tool.call`, `connector.tool.result`
2. Implement validation + stable error mapping (bad schema => close connection with reason).
Acceptance:
1. Invalid messages do not crash the gateway and are rejected deterministically.
2. All accepted messages are persisted/audited with correlation ids.

### CONN-004 WebSocket Transport (Outbound-Only Model)
Priority: P0  
Files: `crates/carsinos-gateway/src/main.rs`  
Deliver:
1. Add `GET /api/v1/connectors/ws` WebSocket endpoint.
2. Auth model:
   - connector presents `Authorization: Bearer <token>`
   - gateway matches token against runtime-configured `token_secret_ref` for claimed `connector_id`
3. Maintain in-memory session map keyed by connector_id for dispatch.
Acceptance:
1. Connector connects from behind NAT (outbound WS only).
2. Gateway exposes connector presence in status endpoint and emits events.

### CONN-005 Tool Dispatch Bridge (Gateway -> Connector)
Priority: P0  
Files: `crates/carsinos-gateway/src/main.rs`  
Deliver:
1. Add a gateway internal API to enqueue a connector tool call (by connector_id).
2. Add allowlist enforcement:
   - connector must be enabled
   - tool_name must be allowlisted for connector
   - risk class must be compatible with approvals policy
3. Add idempotency for tool calls (retry without duplicate execution where possible).
Acceptance:
1. A call can be dispatched to a connected connector and returns result.
2. Disconnected connector yields queued or rejected behavior (explicit, not silent failure).

### CONN-006 Windows Connector Daemon (Headless)
Priority: P0  
Files: new folder under `crates/` (suggest: `crates/carsinos-connector-windows/` + shared `crates/carsinos-connector-core/`)  
Deliver:
1. Connector core client: reconnect loop + heartbeat + message handling.
2. Windows daemon implementation supports:
   - `tool.exec` with local allowlisted binaries + working dirs
   - output clipping + duration measurement
3. Local config file for allowlists (no remote override in P0).
Acceptance:
1. Demo end-to-end: dispatch a bounded exec from gateway and get result + audit.
2. Daemon refuses non-allowlisted commands even if gateway requests them.

### CONN-007 P0 Test Gate
Priority: P0  
Files: `crates/carsinos-gateway/tests/`, new connector-core tests  
Deliver:
1. Gateway tests for:
   - auth mismatch rejection
   - message validation
   - allowlist enforcement
2. Connector-core tests for:
   - reconnect/backoff
   - output clipping
Acceptance commands:
1. `cargo check -p carsinos-gateway --bin carsinos-gateway`
2. `cargo test -p carsinos-gateway connector_ -- --nocapture` (add tests under this prefix)

## P1 (Mobile + Operator Loop)

### CONN-101 Android Connector (Approvals + Alerts)
Priority: P1  
Deliver:
1. Android app that connects as a connector and can:
   - receive alert events (push later; WS in foreground for MVP)
   - resolve approvals (approve/deny) with strong auth
2. Use RMP approach (Rust core + UniFFI) or a thin Kotlin client if faster.
Acceptance:
1. Operator can resolve an approval from Android without desktop.

### CONN-102 Connector Capability Bridge Pattern
Priority: P1  
Deliver:
1. Add platform capability bridges (notifications, local files) with explicit permission prompts.
2. Expand allowlist model to per-capability settings (not just tool names).
Acceptance:
1. No capability executes without both: gateway allowlist AND local allowlist.

## P2 (Channel Connectors)

### CONN-201 Channel Adapter Connectors (Future)
Priority: P2  
Deliver:
1. Treat “hard channels” as connector capability bundles:
   - iMessage, WhatsApp, Signal, Slack, etc.
2. Standardize message ingest contract into carsinOS sessions/runs.
Acceptance:
1. New channel adapters can be added without changing gateway core logic (capability plugin model).

## Execution Guardrails
1. Outbound-only connectors (no inbound firewall holes).
2. Double allowlist enforcement (gateway + device).
3. All connector tool calls are auditable and reversible where applicable.
4. Keep P0 tiny: 1 tool (`tool.exec`) + 1 platform (Windows) proves the path.

