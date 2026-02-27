# carsinOS Connector Apps (RMP-Inspired) Spec

Date: 2026-02-27  
Owner: AppDex  
Status: Draft (implementation-ready outline)

## What We’re Building
Add “connector apps” that run on other devices (Windows PC, Android phone, later: iOS) and safely extend carsinOS with remote capabilities:
- operator surfaces on mobile (approvals, alerts, quick actions)
- remote tool execution on trusted machines (bounded + audited)
- future channel adapters that are hard to run centrally (iMessage, WhatsApp, etc.)

This uses the **RMP (Rust Multi-Platform)** approach from `justinmoon/rmp-example`: a shared Rust core for protocol/state/networking, plus thin native shells per platform.

## Non-Goals (MVP)
- No attempt to reuse the current Electron Mission Control UI.
- No “magic remote root shell”. Remote exec must be allowlisted, permissioned, and auditable.
- No requirement to push-connect into devices (devices should initiate outbound connections).

## Why RMP Here (Pragmatic)
The `rmp-example` repo is not a connector protocol; it’s an architecture/pattern:
- Rust owns protocol, transport, and security-critical behavior
- platforms only render UI and provide bounded capability bridges
- unidirectional state flow (TEA/Elm) makes reconnection/retry logic reliable
- UniFFI enables Kotlin/Swift bindings for mobile without hand-written JNI/FFI

## High-Level Architecture
1. **carsinOS Gateway (“Connector Hub”)**: maintains connector registry, auth, sessions, command queue, audit.
2. **Connector Core (Rust crate)**: shared client library that speaks the connector protocol, handles reconnect, executes allowlisted capabilities, and emits structured results.
3. **Platform Shells**:
   - Windows: background service + optional tray UI (status + allowlist config)
   - Android: Compose UI + background worker (approvals + notifications + optional capabilities)

### Connection Model (Outbound Only)
Connectors always initiate the connection to the gateway (works behind NAT, works on mobile).
- Transport: WebSocket preferred; fallback long-poll (MVP can start with long-poll).
- Gateway never needs inbound access to the device.

## Connector Protocol (Minimal v1)
All messages are typed and versioned. Start with JSON for speed; leave MessagePack for later.

### Handshake
Connector -> Gateway:
- `connector.hello`:
  - `schema_version`: `connector.v1`
  - `connector_id`: stable device id
  - `platform`: `windows|android|...`
  - `capabilities`: list of named operations + local policy metadata
  - `build`: version/commit

Gateway -> Connector:
- `connector.welcome`:
  - `session_id`
  - `server_time`
  - `policy`: effective allowlists/denies (what gateway expects the connector to enforce locally)

### Heartbeat / Presence
Connector -> Gateway:
- `connector.heartbeat` every N seconds with:
  - `session_id`, `uptime_ms`, `battery` (mobile), `network` (optional), `last_error` (optional)

### Remote Tool Call (Pull or Push)
Gateway -> Connector:
- `connector.tool.call`:
  - `call_id`
  - `tool_name` (ex: `tool.exec`, `fs.read`)
  - `args` (typed per tool)
  - `risk` + `requires_approval` (gateway-side classification)

Connector -> Gateway:
- `connector.tool.result`:
  - `call_id`, `status`, `stdout/stderr` (clipped), `artifacts` refs (if any)
  - `duration_ms`, `truncated` flags

## Security Requirements (Hard)
1. **Connector auth is token-based** (JWT or opaque bearer; align with carsinOS auth model).
2. **Defense-in-depth**:
   - gateway enforces per-connector capability allowlist
   - connector enforces local allowlist (binaries/paths/network) even if gateway is compromised
3. **Audit everything**:
   - every `connector.tool.call` and result gets an audit row with correlation ids
4. **No secret leakage**:
   - connector never logs tokens
   - gateway never returns tokens once stored (write-only secrets)
5. **Kill switches**:
   - per-connector disable
   - global disable of connector tool execution

## carsinOS Gateway Work (MVP)
Add a “Connector Hub” module with:
- connector registry (id, label, platform, status, last_seen)
- connector sessions (connected, heartbeat, last_error)
- command queue (pending/running/completed; retries + idempotency)
- APIs:
  - `POST /api/v1/connectors/register` (or provision out-of-band)
  - `GET /api/v1/connectors` (status list)
  - `GET /api/v1/connectors/{id}` (detail)
  - `POST /api/v1/connectors/{id}/disable|enable`
  - `WS /api/v1/connectors/ws` (primary transport)
- event stream:
  - `connector.connected`, `connector.disconnected`, `connector.heartbeat`, `connector.tool.*`

## Connector Apps (MVP Targets)
### Windows Connector (P0)
Goal: safe remote execution for a known machine.
- headless daemon that connects to gateway and executes allowlisted `tool.exec`
- config:
  - allowed binaries list
  - allowed working directories
  - max output size

### Android Connector (P0/P1)
Goal: “mission control in your pocket”.
- receive approvals + incident alerts
- allow “approve/deny” actions with strong authentication
- optional: limited tool calls (avoid anything that turns phone into a shell)

## Suggested Phasing
P0:
- Gateway connector hub skeleton + WS transport + heartbeat + registry
- Windows connector daemon with `tool.exec` (bounded)
- Android connector “approvals + notifications” only

P1:
- richer capability bridge (files, screenshots, device info) with explicit allowlists
- mobile background reliability (FCM/APNS integration if needed)

P2:
- channel adapter connectors (iMessage/WhatsApp/etc.) as separate capability bundles

## Acceptance Criteria (MVP)
1. A connector can connect, heartbeat, and appear in gateway status within 5 seconds.
2. Gateway can dispatch a bounded `tool.exec` to Windows connector; result is returned + audited.
3. Android connector can receive and resolve an approval without desktop intervention.
4. Disabling a connector immediately prevents further tool dispatch.
5. No tokens/secrets appear in logs or API responses.

