# AppDex Executable Ticket Backlog: Slick Mission Control + Agent Mail (carsinOS)

Date: 2026-02-27  
Owner: AppDex  
Scope: Ship a Trello-grade Mission Control UI (matching `missioncontrol/pipeline/*.png`) **and** bring forward the best “original MC” cockpit capabilities (widgets/layout studio/multi-profile ops), plus an agent-to-agent comms layer inspired by `mcp_agent_mail`.

## North Star (Plain-English)
Mission Control becomes a daily-driver “command center”:
- Trello-like boards for Tasks + Content Pipeline, live-updating.
- Calendar that shows scheduled/autonomous jobs in a week view you can trust.
- Operator focus: what needs attention right now (approvals, failures, disconnected channels, budget breaches).
- Dual-agent by default: Lyra + Claude are both active and visible, with no hidden “active profile” confusion.
- Agents can coordinate without the human acting as messenger: chatrooms + mail threads + searchable history.

## Hard Constraints
1. **New UI**: do not reuse the current Electron MissionControl UI.
2. **Rust speed**: use **Tauri (Rust) + React** (WebView UI; heavy logic stays in the Rust gateway).
3. **Backend stays carsinOS**: Mission Control is a client of `carsinos-gateway` over HTTP + WS (`/api/v1/ws`).
4. **Guardrails-first**: no auto-run loops, no unbounded retries, budgets and breakers remain enforced.
5. **Config-first**: no hardcoded runtime values for primary agents, endpoints, profile IDs, limits, or feature flags.

## Execution Lock Additions (Mandatory)
### 1) API Contract Freeze (Before P0 Buildout)
Freeze and version these interfaces before UI implementation:
1. Board HTTP contracts used by P0 (`board list/detail/card create/update/move/run`, `asset upload/download`).
2. WS event contract for UI patches:
   - envelope: `event_id`, `event_type`, `ts_unix_ms`, `request_id?`, `entity`, `payload`, `schema_version`.
   - required types at minimum: `board.card.created|updated|moved|deleted`, `run.updated`, `approval.requested|resolved`, `job.updated`, `channel.status.changed`.
3. P1 read models: calendar week payload + operator-focus queue payload.
4. P3 Agent Mail APIs + WS events (`agent_mail.thread.*`, `agent_mail.message.created`, `agent_mail.message.ack`).
5. Stable error taxonomy for all new routes (`INVALID_INPUT`, `AUTH_REQUIRED`, `AUTH_FORBIDDEN`, `RATE_LIMITED`, `CONFLICT`, `NOT_FOUND`, `INTERNAL_ERROR`).

### 2) Config-First Policy (No Hardcoded Values)
All operator and deployment-specific values must come from config/UI settings:
1. Gateway URL, auth token source, reconnect policy, WS backoff, UI refresh intervals.
2. Primary agent IDs (Lyra/Claude defaults are initial config values, not code literals).
3. Upload limits, attachment allowlist, preview limits, thread/message rate limits.
4. Feature flags by phase (`slick_ui_enabled`, `agent_mail_enabled`, `mcp_mail_enabled`).
5. Migration toggles and rollout mode (`legacy_mc_fallback`, `readonly_shadow_mode`).

### 3) Security Baseline for P0/P1/P3
1. Attachment controls: MIME allowlist, size cap, filename/path sanitization, checksum-at-ingest.
2. Asset and mail downloads must enforce authz and ownership checks.
3. Anti-abuse: per-principal and per-thread quotas for Agent Mail and chatrooms.
4. Audit events required for mutation endpoints and policy denials.
5. No secret/token values in UI logs, browser console, or persisted plaintext.

### 4) Migration and Rollout Safety
1. Non-destructive schema migrations only (`CREATE TABLE IF NOT EXISTS`, additive columns/indexes).
2. Preserve existing MC/runtime data paths; no destructive rewrite in-place.
3. Add rollback plan per phase (feature flag off + API compatibility maintained).
4. Shadow-mode rollout for P0/P1 screens before defaulting new MC as primary.

### 5) Regression + Performance Gates (Per Phase)
At the end of each major phase, all must pass:
1. Backend regression:
   - `cargo test -p carsinos-gateway`
   - `cargo test -p carsinos-storage`
   - `cargo test -p carsinos-tools`
   - `cargo test -p carsinos-gateway --test e2e_process`
2. Security gate:
   - `scripts/security_pr_gate.sh`
3. UI gate (new app):
   - typecheck + lint + unit/integration + production build.
4. Performance budgets:
   - P0 card move reflected on second client within 1s (p95 local).
   - P0 board interactions remain responsive at 500+ cards.
   - P3 mail search remains responsive at 50k messages.

## Phase Order (Strict)
1. **P0**: Slick Trello boards (daily driver), realtime, assets, dual-agent dispatch.
2. **P1**: Calendar week view + Operator Focus + event stream noise controls.
3. **P2**: “Original MC” cockpit carryover: widget system + Layout Studio + health strip + deep ops.
4. **P3**: Agent-to-agent comms: chatrooms + mail threads + attachments + search (+ optional file leases).
5. **P4**: Optional MCP-compat layer (expose Agent Mail via MCP for external agent tools).

## Gates (Stop/Go)
### Gate A (P0): Trello-Grade Daily Driver
Pass when:
1. New MC app launches (`tauri dev`), connects to gateway, stays stable for 1 hour.
2. Tasks + Content boards support: create/edit, **drag/drop move**, reorder, assignee (Lyra/Claude), tags.
3. Card drawer shows script, assets, linked run history; “Run card” works and updates state.
4. **Realtime**: two MC clients see board changes within 1s via WS (no manual refresh loop).
5. Asset upload from UI works; previews render (no exposing raw server filesystem paths).

### Gate B (P1): “Trustable Calendar” + Operator Focus
Pass when:
1. Calendar shows a **week view** + “Always Running” lane + “Next Up”.
2. Jobs can be paused/resumed/run-now from UI; state matches gateway.
3. Operator Focus queue is usable (approvals, failures, disconnected channels, breaker opens).
4. Event stream is readable: heartbeat noise filtered by default; “show raw” toggle exists.

### Gate C (P2): Cockpit + Layout Studio (Best of Original MC)
Pass when:
1. Widget palette + grid layout + saved pages/presets exist.
2. Pinned health strip is always visible (budgets, breakers, channel health, approvals count).
3. Import/export layouts (JSON) works.
4. Multi-agent control matrix exists (Lyra/Claude routing, auth profile order, channel ops, plugin/skill toggles).

### Gate D (P3): Agent Mail + Chatrooms
Pass when:
1. Lyra can message Claude and vice versa via Agent Mail (and a human can watch in MC).
2. Messages are searchable and threaded; attachments supported.
3. Guardrails prevent ping-pong loops: no auto-run on inbound messages by default; rate limits exist.

## P0 Tickets (Slick Mission Control App + Boards)

### MC-SLICK-BE-000 Contracts + Conformance Freeze
Priority: P0  
Goal: eliminate API churn risk before UI acceleration.  
Deliver:
1. Publish frozen schemas for P0 routes + WS board/run/approval/job payloads.
2. Add contract conformance tests for all frozen routes/events.
3. Publish versioning/deprecation rule (`v1` additive-only during P0/P1).
Acceptance:
1. Contract tests are green and required in CI.
2. UI team can implement without rework from payload drift.

### MC-SLICK-APP-001 Create Tauri + React App Skeleton
Priority: P0  
Goal: Start a real UI track that can reach Trello polish fast.  
Deliver:
1. Add `apps/mission-control/` (or similar) with Tauri + React + Vite.
2. Dev scripts for one-command local run.
Acceptance:
1. `npm install` + `npm run dev` works for UI.
2. `npm run tauri dev` launches desktop app and renders shell.

### MC-SLICK-APP-002 Connection + Auth UX (Gateway URL + Token)
Priority: P0  
Deliver:
1. Connection panel (gateway base URL, bearer token).
2. Store token in OS keychain (never echo back once set).
3. Health indicator + reconnect action.
4. Settings-backed config (no hardcoded endpoint/auth values).
Acceptance:
1. Operator can change gateway URL/token without editing files.
2. Token is never printed in logs.

### MC-SLICK-APP-003 Realtime Client (WS Event Bus)
Priority: P0  
Deliver:
1. Connect to `/api/v1/ws`.
2. Parse event envelope and route by `event_type` (`board.*`, `run.*`, `approval.*`, `job.*`, etc.).
3. UI state updates are incremental (no full reload per event).
Acceptance:
1. Card move from Client A appears on Client B without refresh.

### MC-SLICK-APP-010 Board UI (Virtualized Columns + Drag/Drop)
Priority: P0  
Deliver:
1. Kanban board component with column virtualization and card virtualization.
2. Drag/drop between columns + reorder within a column.
3. Optimistic update + rollback on API error.
Acceptance:
1. 500+ cards does not freeze the UI.

### MC-SLICK-APP-011 Card Drawer (Fields + Run Hook)
Priority: P0  
Deliver:
1. Drawer: description, owner (agent/human from config-backed agent list), due date, tags, script markdown.
2. “Run card” button; shows run status + latest run id.
Acceptance:
1. Run is triggered via `POST /api/v1/boards/{board}/cards/{card}/run`.
2. Drawer shows updated linked run id within 2s (WS preferred).

### MC-SLICK-APP-012 Asset Upload + Preview
Priority: P0  
Deliver:
1. Upload UI (drag/drop file, mime sniffing, size cap).
2. Previews (images, svg, basic file list).
Acceptance:
1. Upload uses gateway endpoint; UI can fetch the asset back for preview (no raw `local_path` usage).
2. MIME + size policy enforced from config and mirrored in backend validation.

### MC-SLICK-BE-001 Serve Board Assets Safely
Priority: P0  
Goal: UI must not rely on server filesystem paths.  
Deliver:
1. Add `GET /api/v1/boards/{board_id}/cards/{card_id}/assets/{card_asset_id}` returning bytes + correct `Content-Type`.
2. Ensure auth + board/card/asset ownership validation.
3. Enforce filename/path sanitization and content-type/size guardrails.
Acceptance:
1. UI previews work from a different machine (not just localhost).

### MC-SLICK-BE-002 Board WS Payloads (UI-Diff Friendly)
Priority: P0  
Deliver:
1. Ensure emitted events include enough fields for incremental patching (card id, column id, positions, updated fields).
2. Document event shapes in `carsinos-protocol`.
Acceptance:
1. UI can update a moved/edited card without re-fetching entire board.

## P1 Tickets (Calendar + Operator Focus + Noise Controls)

### MC-SLICK-BE-101 Calendar + Focus Read Models
Priority: P1  
Deliver:
1. Stable week-view API model (always running lane + next-up queue).
2. Stable operator-focus API model (approvals/failures/breakers/channels with primary action hints).
3. Contract tests for read-model fields consumed by UI.
Acceptance:
1. UI renders week + focus without requiring ad hoc joins on client side.

### MC-SLICK-APP-101 Calendar Week View
Priority: P1  
Deliver:
1. Week grid view + “Always Running” lane + “Next Up” queue.
2. Actions: run-now, pause/resume, view history.
Acceptance:
1. Operator can visually verify the scheduler is doing what it should.

### MC-SLICK-APP-102 Operator Focus Queue
Priority: P1  
Deliver:
1. Unified queue: approvals requested, runs failed, breakers opened, channels disconnected.
2. Each item has 1-click primary action (approve/deny/retry/reconnect).
Acceptance:
1. You can clear a “bad state” without hunting across tabs.

### MC-SLICK-APP-103 Event Stream (Noise Controls)
Priority: P1  
Deliver:
1. Live event stream backed by WS.
2. Default filters hide heartbeat spam; toggle shows raw.
Acceptance:
1. Stream remains readable under load.

## P2 Tickets (Original MC “Cockpit” Carryover)
Reference: `missioncontrol/docs/MISSION_CONTROL_V2_DESIGN_PLAN.md` + `missioncontrol/docs/MC_MULTI_PROFILE_COCKPIT_EXECUTION_SPEC.md`.

### MC-SLICK-APP-201 Cockpit Widget Framework
Priority: P2  
Deliver:
1. Widget registry + widget palette.
2. Grid layout with resize/drag handles and min/max constraints per widget.
3. Saved pages/presets + import/export JSON.
Acceptance:
1. Operator can build 3 custom pages and restore defaults.

### MC-SLICK-APP-202 Pinned Health Strip + Incident Mode
Priority: P2  
Deliver:
1. Always-visible health strip: gateway online, approvals, budgets, breakers, channels.
2. “Incident mode” that surfaces only what’s broken + recovery actions.
Acceptance:
1. “Am I safe to run?” answered in < 3 seconds.

### MC-SLICK-APP-203 Multi-Agent / Multi-Provider Control Surfaces
Priority: P2  
Deliver:
1. Per-agent provider profile order editor (Lyra/Claude).
2. Channel ops surfaces (probe/pause/resume/reconnect) and reason display.
3. Skills + plugins list + enable/disable + status.
Acceptance:
1. No “SSH into the box” required for routine ops.

## P3 Tickets (Agent Mail + Chatrooms, Inspired by mcp_agent_mail)

### AM-BE-001 Agent Mail Persistence + Search
Priority: P3  
Deliver:
1. DB tables for: threads, messages, recipients/read state, attachments metadata.
2. Search (SQLite FTS) on subject/body.
3. Attachment policy tables/config wiring (MIME allowlist, size caps, retention metadata).
Acceptance:
1. “Search thread history” is fast on 50k messages.

### AM-BE-002 Agent Mail APIs
Priority: P3  
Deliver:
1. Threads CRUD + send message + inbox/outbox + ack.
2. Attachment upload/download (same safety posture as board assets).
Acceptance:
1. Lyra can send message to Claude; Claude inbox shows it immediately.

### AM-BE-003 Chatrooms as Threads (Realtime)
Priority: P3  
Deliver:
1. “Room threads” flagged as `kind=room`.
2. WS events `agent_mail.message.created` for live room updates.
Acceptance:
1. MC chatroom view updates live.

### AM-BE-004 Guardrails (Anti-Spam / Anti-Loop)
Priority: P3  
Deliver:
1. Rate limits per agent + per thread.
2. Default: inbound agent-mail never auto-triggers a run.
3. Optional: “actionable message” type requires explicit operator approval before a run.
4. Audit events for send/ack/deny/rate-limit/policy-deny paths.
Acceptance:
1. Two agents cannot accidentally ping-pong into runaway activity.

### AM-APP-001 Mission Control “Mail” Page
Priority: P3  
Deliver:
1. Inbox/outbox, thread list, thread viewer, compose.
2. Search UI and “summarize thread” action (summary stored as a note).
Acceptance:
1. Operator can monitor + intervene without copy/paste relay.

### AM-APP-002 Mission Control “Chatrooms” Page
Priority: P3  
Deliver:
1. Room list + live chat view with reactions/basic moderation.
2. Agent status chips in-room (Lyra/Claude + others).
Acceptance:
1. Feels like a lightweight internal Slack channel.

### AM-OPT-001 Advisory File Leases (Optional)
Priority: P3 (optional)  
Deliver:
1. Agents can reserve file globs with TTL and “exclusive” flag.
2. UI surfaces active leases + conflicts.
Acceptance:
1. Helps prevent agents stepping on the same files during parallel work.

## P4 (Optional): MCP Compatibility Layer

### AM-MCP-001 MCP Server Facade for Agent Mail (Optional)
Priority: P4  
Goal: Allow external coding agents to use Agent Mail via MCP tools/resources.  
Deliver:
1. Implement MCP server in Rust (HTTP transport) that proxies to Agent Mail APIs.
2. Tools: register identity, send, fetch inbox, ack, reserve files.
Acceptance:
1. An MCP client can send/receive without any Mission Control UI involved.

## Notes on “Rust Speed”
Tauri gives a Rust-native shell; React is for rendering only.
Keep “heavy” work in Rust (`carsinos-gateway`): search, filtering, diff generation, audit, policy checks.
Use virtualization for boards and incremental WS patches to avoid JS bottlenecks.

## PR Chunking + Workflow
1. PR-A: `MC-SLICK-BE-000`, `MC-SLICK-BE-001`, `MC-SLICK-BE-002`.
2. PR-B: `MC-SLICK-APP-001..003`, `MC-SLICK-APP-010..012`.
3. PR-C: `MC-SLICK-BE-101`, `MC-SLICK-APP-101..103`.
4. PR-D: `MC-SLICK-APP-201..203`.
5. PR-E: `AM-BE-001..004`, `AM-APP-001..002`, optional `AM-OPT-001`.
6. PR-F (optional): `AM-MCP-001`.

Each PR follows:
1. checkpoint update (phase start),
2. implementation,
3. green regression/security gates,
4. checkpoint update (post-green),
5. PR open,
6. CodeRabbit review loop,
7. merge,
8. checkpoint update (post-merge).
