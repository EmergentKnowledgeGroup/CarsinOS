# Codebase E2E Audit (Master)

- repo: `carsinos`
- branch: `codex/mc-onboarding-wizard`
- head: `a5c9047`
- started_at_utc: `2026-03-01`
- scope:
  - Rust workspace crates (`crates/*`)
  - Mission Control app (`apps/mission-control/*`, incl. Tauri)
  - scripts, migrations, runtime contracts (as they affect connectors/e2e)

> Operating rule for this audit: **no implementation changes** (code fixes/features). This document is the single master audit log; record findings + evidence inline as the review progresses.

## 0) Audit log (chronological)

- 2026-03-01: Phase start. Checkpoint created/updated under `runtime/checkpoints/` (`LATEST.md`, `LATEST.json`, and `context_checkpoint_*_codebase-audit.*`).
- Next: repo inventory + e2e connector map.
- 2026-03-01: Repo scan: no `TODO`/`FIXME`/`HACK` markers found (`rg -n "TODO|FIXME|HACK" -S .`).

## 1) System map (e2e mental model)

### Surfaces (operator/user-facing)
- `apps/mission-control`: Mission Control UI (web/tauri).
- `crates/carsinos-gui`: egui GUI client.
- `crates/carsinos-cli`: CLI client.

### Backend composition root
- `crates/carsinos-gateway`: axum HTTP/WS gateway server (API surface + background supervisors).

### Core domain + supporting layers
- `crates/carsinos-core`: agent loop + approvals + tool/runtime policies + channel/runtime orchestration (expected).
- `crates/carsinos-protocol`: API + WS types (schema/versioning boundary).
- `crates/carsinos-storage`: SQLite storage layer + repositories.
- `crates/carsinos-providers`: model provider adapters (OpenAI/Anthropic/etc).
- `crates/carsinos-tools`: tool implementations (`exec`, `fs.*`, `web.*`, etc).
- `crates/carsinos-channels-*`: per-channel adapters (Discord/Telegram/etc).
  - Note: gateway runtime currently wires `discord` + `telegram` adapters; other channel crates (Slack/Signal/Twitch/WhatsApp/BlueBubbles) appear to be present as scaffolds + unit tests but are not referenced by `carsinos-gateway` imports/routes.

### Entry points (where execution starts)
- Gateway server: `crates/carsinos-gateway/src/main.rs` (`#[tokio::main] async fn main()`)
- CLI: `crates/carsinos-cli/src/main.rs`
- egui GUI client: `crates/carsinos-gui/src/main.rs`
- Mission Control web: `apps/mission-control/src/main.tsx` → `App.tsx`
- Mission Control Tauri shell: `apps/mission-control/src-tauri/src/main.rs` → `src/lib.rs`

### Ops / automation entrypoints (scripts)
- Packaging: `scripts/package_macos_app.sh` (invoked by `carsinos-cli package-macos`)
- Security gates/runbooks automation: `scripts/security_pr_gate.sh`, `scripts/security_nightly_deep_scan.sh`, `scripts/security_killswitch_drill.sh`, `scripts/security_secret_lifecycle_drill.sh`, `scripts/security_gate0_evidence_bundle.sh`, `scripts/security_archive_retention_proof.sh`
- Channel soak harness: `scripts/channel_soak_runner.py` (+ unit tests under `scripts/tests/`)
- Mission Control Tauri dev launcher: `scripts/launch_mission_control_tauri_dev.command`

## 2) Connector inventory (to verify)

| Connector | Direction | Transport | AuthN/AuthZ | Config source | Primary code pointers | Test coverage | Audit status |
|---|---|---|---|---|---|---|---|
| Mission Control ↔ Gateway | UI → backend | HTTP + WS | Bearer token (static or JWT); WS supports `?token=` fallback | `localStorage` + (Tauri keychain if present) | `apps/mission-control/src/lib/{api.ts,ws.ts,runtime.ts}`; gateway: `crates/carsinos-gateway/src/main.rs` (`build_app`, `ws_handler`) | Gateway process E2E tests for HTTP+WS auth/events; MC UI has no automated tests | **in-progress** |
| Tauri ↔ MC frontend | desktop shell | Tauri invoke commands | n/a (local) | macOS keychain via `keyring` | `apps/mission-control/src-tauri/src/lib.rs` (`set_gateway_token`, `get_gateway_token`, etc) + `apps/mission-control/src/lib/runtime.ts` | none found | **in-progress** |
| Gateway ↔ SQLite | backend → data | rusqlite | n/a | `CARSINOS_STATE_DIR` + `directories` default | gateway init: `crates/carsinos-gateway/src/main.rs` (startup path); schema: `migrations/0001_init.sql`; impl: `crates/carsinos-storage/src/lib.rs` | gateway process E2E tests cover persistence across restart | **in-progress** |
| Gateway ↔ Providers | backend → external | HTTPS (reqwest) | auth profiles + secret refs | auth profiles + runtime provider policies + `SecretStore` | providers: `crates/carsinos-providers/src/lib.rs`; gateway composition: `crates/carsinos-gateway/src/main.rs` (ProviderRegistry + run loop + auth profile endpoints) | provider unit tests + gateway auth/profile/OAuth/run-flow tests | **in-progress** |
| Gateway ↔ Tools runtime | backend → local ops | subprocess/fs/net | approval-gated | env sandbox policy + runtime guardrails | tools: `crates/carsinos-tools/src/lib.rs`; gateway: `ToolRegistry` + `execute_run` tool loop + `/api/v1/tools/capabilities` | tools unit tests + gateway tool approval + process E2E tests | **in-progress** |
| Gateway ↔ Channels | backend → external | per-channel (ureq transport clients; long poll/webhook) | allowlist/mention gating + operator allowlist | runtime config `channels.*` + secret refs via `SecretStore` | gateway: `ChannelRuntimeManager`, `DiscordRuntimeAdapter`, `TelegramRuntimeAdapter`, `/api/v1/channels/*` endpoints (all in `crates/carsinos-gateway/src/main.rs`); transports: `crates/carsinos-channels-{discord,telegram}/src/lib.rs` | channel crate unit tests + gateway listener/ingest tests + process soak harness | **in-progress** |
| Gateway ↔ Numquam (if enabled) | backend → external | HTTP or MCP (or dual) | secret-backed | env + runtime config | gateway: `NumquamClient`, `resolve_numquam_client`, handshake loop in `crates/carsinos-gateway/src/main.rs`; protocol types in `crates/carsinos-protocol/src/lib.rs` | gateway unit + process tests for Numquam http+mcp + degrade modes | **in-progress** |

## 3) Feature inventory (to verify)

### Mission Control (apps/mission-control)
- **Top-level tabs** (source: `apps/mission-control/src/app/tabs.ts`):
  - Boards (`tab="boards"`)
  - Calendar (`tab="calendar"`)
  - Focus (`tab="focus"`)
  - Events (`tab="events"`)
  - Mail (`tab="mail"`)
  - Rooms (`tab="chatrooms"`)
  - Team (`tab="team"`)
  - Cockpit (`tab="cockpit"`)

- **Connector + state orchestration** (source: `apps/mission-control/src/App.tsx`):
  - Runtime connection (gateway URL + bearer token) is managed by `useRuntimeConnectionController` (loads `GET /health`, `GET /boards`, `GET /agents` as baseline).
  - WS event stream is managed via `useGatewayEvents` → `lib/ws.ts` (`GET /api/v1/ws`), used to:
    - append to local event stream (max 400),
    - trigger Mission Control refresh on `job.*`, `approval.*`, `channel.*`, `extension.*`,
    - trigger Agent Mail refresh on `agent_mail.*`,
    - apply board-specific events in-place (`board.*`).

- **API client single source-of-truth**: `apps/mission-control/src/lib/api.ts` (all HTTP calls + WS URL builder).

- **Boards tab**:
  - HTTP deps: `GET /api/v1/boards`, `GET /api/v1/boards/{board_id}`, card CRUD/move/run, asset upload/download (see `lib/api.ts` + `features/boards/useBoardsController.ts`).
  - WS deps (event types): `board.card.created`, `board.card.updated`, `board.card.moved`, `board.card.run`, `board.asset.uploaded` (see `features/boards/useBoardsController.ts`).
  - Cross-check: gateway emits those event types with expected `payload` fields (source: `crates/carsinos-gateway/src/main.rs` around `create_board_card`/`update_board_card`/`move_board_card`/`upload_board_card_asset`/`run_board_card`).

- **Calendar tab** (source: `features/calendar/CalendarPage.tsx` + `app/useMissionControlController.ts`):
  - HTTP deps: `GET /api/v1/mission-control/calendar/week`, `GET /api/v1/jobs`, `GET /api/v1/jobs/status`, `POST /api/v1/jobs/{job_id}/run`, `POST /api/v1/jobs/{job_id}/update`.

- **Focus tab**:
  - HTTP deps: `GET /api/v1/mission-control/focus`, `GET /api/v1/approvals`, `POST /api/v1/approvals/{approval_id}/resolve`, `GET /api/v1/channels/runtime/status`, `POST /api/v1/channels/runtime/reconnect`.

- **Events tab**:
  - Data source: WS event stream only (no direct HTTP calls in view layer).

- **Mail + Rooms tabs** (source: `features/agentMail/*` + `features/agentMail/useAgentMailController.ts`):
  - HTTP deps: agent-mail threads/messages/attachments/ack; file leases; plus `POST /api/v1/memory/notes` for “summarize to note” action (verify in controller).

- **Team tab**:
  - Data source: agents list (baseline) + Mission Control read models (jobs/approvals/status).

- **Cockpit tab**:
  - Data source: Mission Control read models + WS stream.
  - HTTP deps (via Mission Control controller): skills/plugins list + enable/disable, provider profile order get/set, etc.

- **Onboarding wizard** (source: `features/onboarding/*`):
  - HTTP deps: OpenAI OAuth start/finish; Anthropic setup-token ingest; auth profiles list.

### Gateway + core loop (Rust crates)
- **HTTP API surface** (source: `crates/carsinos-gateway/src/main.rs` → `build_app()`):
  - health/status/metrics: `GET /api/v1/health`, `GET /api/v1/status`, `GET /api/v1/metrics`
  - capabilities:
    - `GET /api/v1/providers/capabilities`
    - `GET /api/v1/tools/capabilities`
  - extensions:
    - `GET /api/v1/extensions/plugins`
    - `GET /api/v1/extensions/plugins/status`
    - `POST /api/v1/extensions/plugins/install`
    - `POST /api/v1/extensions/plugins/{plugin_id}/update`
    - `POST /api/v1/extensions/plugins/{plugin_id}/rollback`
    - `GET /api/v1/extensions/skills`
    - `POST /api/v1/extensions/skills/{skill_id}/state`
  - agents:
    - `GET /api/v1/agents`
    - `POST /api/v1/agents`
    - `GET /api/v1/agents/{agent_id}`
    - `POST /api/v1/agents/{agent_id}` (update)
  - boards:
    - `GET /api/v1/boards`
    - `GET /api/v1/boards/{board_id}`
    - `POST /api/v1/boards/{board_id}/cards/create`
    - `POST /api/v1/boards/{board_id}/cards/{card_id}/update`
    - `POST /api/v1/boards/{board_id}/cards/{card_id}/move`
    - `POST /api/v1/boards/{board_id}/cards/{card_id}/assets/upload`
    - `GET /api/v1/boards/{board_id}/cards/{card_id}/assets/{card_asset_id}`
    - `POST /api/v1/boards/{board_id}/cards/{card_id}/run`
    - `GET /api/v1/boards/{board_id}/automation`
    - `GET /api/v1/boards/{board_id}/automation/{job_id}`
    - `POST /api/v1/boards/{board_id}/columns/{column_id}/automation/upsert`
    - `POST /api/v1/boards/{board_id}/automation/{job_id}/state`
    - `POST /api/v1/boards/{board_id}/automation/{job_id}/run`
  - websocket:
    - `GET /api/v1/ws`
  - sessions/runs:
    - `GET /api/v1/sessions`
    - `POST /api/v1/sessions`
    - `GET /api/v1/sessions/{session_id}`
    - `GET /api/v1/sessions/{session_id}/messages`
    - `POST /api/v1/sessions/{session_id}/messages`
    - `POST /api/v1/sessions/{session_id}/runs`
    - `POST /api/v1/runs/{run_id}/resume`
    - `POST /api/v1/runs/{run_id}/memory/why`
  - memory:
    - `GET /api/v1/memory/notes`
    - `POST /api/v1/memory/notes`
    - `GET /api/v1/memory/notes/{note_id}`
    - `POST /api/v1/memory/notes/{note_id}` (update)
    - `POST /api/v1/memory/search`
    - `POST /api/v1/memory/sync`
  - auth:
    - `GET /api/v1/auth/profiles`
    - `POST /api/v1/auth/profiles`
    - `POST /api/v1/auth/profiles/{auth_profile_id}/state`
    - `POST /api/v1/auth/openai/oauth/start`
    - `POST /api/v1/auth/openai/oauth/finish`
    - `POST /api/v1/auth/anthropic/setup-token/ingest`
    - `GET/POST /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order`
  - config:
    - `GET/POST /api/v1/config/channels`
    - `GET/POST /api/v1/config/runtime`
    - `GET /api/v1/config/runtime/trust-lock`
    - `POST /api/v1/config/runtime/trust-lock/refresh`
    - `POST /api/v1/config/runtime/rollback`
    - `POST /api/v1/config/runtime/secrets/upsert`
    - `POST /api/v1/config/runtime/secrets/delete`
  - channels:
    - `POST /api/v1/channels/telegram/inbound`
    - `POST /api/v1/channels/discord/inbound`
    - `GET /api/v1/channels/runtime/status`
    - `POST /api/v1/channels/runtime/reconnect`
    - `POST /api/v1/channels/approvals/resolve`
  - jobs/scheduler:
    - `GET /api/v1/jobs`
    - `GET /api/v1/jobs/status`
    - `POST /api/v1/jobs/add`
    - `POST /api/v1/jobs/{job_id}/update`
    - `POST /api/v1/jobs/{job_id}/remove`
    - `POST /api/v1/jobs/{job_id}/run`
    - `GET /api/v1/jobs/{job_id}/history`
  - mission-control support endpoints:
    - `GET /api/v1/mission-control/calendar/week`
    - `GET /api/v1/mission-control/focus`
  - agent-mail:
    - `GET/POST /api/v1/agent-mail/threads`
    - `GET /api/v1/agent-mail/threads/{thread_id}`
    - `GET/POST /api/v1/agent-mail/threads/{thread_id}/messages`
    - `POST /api/v1/agent-mail/messages/{message_id}/ack`
    - `POST /api/v1/agent-mail/messages/{message_id}/attachments/upload`
    - `GET /api/v1/agent-mail/messages/{message_id}/attachments/{attachment_id}`
    - `GET/POST /api/v1/agent-mail/leases`
    - `POST /api/v1/agent-mail/leases/{lease_id}/release`
    - `POST /api/v1/agent-mail/mcp`
  - approvals:
    - `GET /api/v1/approvals`
    - `POST /api/v1/approvals/request`
    - `POST /api/v1/approvals/{approval_id}/resolve`
  - security ops:
    - `GET /api/v1/security/audit`
    - `POST /api/v1/security/audit/retention/run`
    - `POST /api/v1/security/auth-profiles/{auth_profile_id}/rotate-secret`
    - `POST /api/v1/security/auth-profiles/{auth_profile_id}/revoke`

- **Extensions model (plugins + skills)** (source: `crates/carsinos-core/src/lib.rs` + gateway init):
  - Plugins:
    - Manifests loaded from disk into `carsinos-core::PluginRegistry` (schema `carsinos.plugin.manifest.v1|v2`, api `carsinos.plugin.api.v1`).
    - Gateway loads manifests from `CARSINOS_PLUGIN_MANIFEST_DIRS` or default `<state_dir>/plugins/active` (see `load_plugin_manifest_dirs_from_env`).
    - Tool-line parsing supports plugin tools by mapping `tool.<capability>` to a `process` request of action `plugin_tool` with `plugin_raw_args` (see `ToolRegistry::parse_line_with_plugins`).
    - Hooks are registered into a `HookBus` from manifest capabilities, subject to `ExtensionSecurityPolicy` allowlist.
    - Hook points emitted by gateway include: `run.start`, `run.end`, `tool.before`, `tool.after`, `compaction.before`, `compaction.after` (see `emit_extension_hook_point`); outcomes are audited via internal security audit events.
    - Daemon-mode plugins (`artifact.exec_kind=daemon`) are additionally gated by runtime config allowlist (`runtime.extensions.plugin_daemon_allowlist`; enforced by `ensure_daemon_allowlist`).
  - Skills:
    - Skills loaded from disk into `carsinos-core::SkillRegistry` (`SkillDocument` with content), from `CARSINOS_SKILL_DIRS` or default `<state_dir>/skills` (see `load_skill_dirs_from_env`).
    - Enabled/disabled overrides are persisted in sqlite (`app_kv`) and applied at startup; reserved IDs/prefixes are blocked from toggling by policy.
    - Run context can inject requested skills when user input includes `@skill:<skill_id>` (bounded by env-configured max skills/chars).

- **Tool execution model (gateway ↔ tools connector)** (source: `crates/carsinos-gateway/src/main.rs` → `execute_run` + `ToolRegistry`):
  - Tool requests are parsed from the **latest user message text** line-by-line; any line starting with `tool.<command>` is interpreted as a tool invocation (see `parse_tool_requests_from_input` + `ToolRegistry::parse_line_with_plugins`).
  - High-risk tools (e.g., `exec`, `fs.write`, `channel.*`) are gated via `approvals`:
    - if approval required: a tool call is persisted as `blocked`, an `approval.requested` WS event is emitted, and the run aborts with an approval-required error until `/api/v1/runs/{run_id}/resume` is called after resolution.
  - Tool execution:
    - core tools execute via `carsinos-tools::LocalToolRunner` inside a bounded concurrency semaphore (`CARSINOS_TOOL_MAX_CONCURRENCY`).
    - plugin tools execute via the plugin runtime supervisor path (`process` tool action `plugin_tool`).
    - channel actions execute via a dedicated handler that bridges to Discord/Telegram transport clients (requires transport mode + secrets).
  - Tool outputs are persisted and appended to the provider prompt as a `Tool outputs:` block before provider completion runs; a short summary is streamed via `run.delta` events.

- **Channel runtime + ingest listener model** (source: `crates/carsinos-gateway/src/main.rs`):
  - Runtime adapters (`DiscordRuntimeAdapter`, `TelegramRuntimeAdapter`) are managed by `ChannelRuntimeManager`, with a supervisor loop calling `reconcile()` every ~10s.
  - Ingest listener loops run in-process:
    - Telegram: when `channels.telegram.operation_mode=transport` and `channels.telegram.webhook_mode=long_poll`, the gateway polls Telegram updates via `TelegramTransportClient.get_updates_once_with_retry` and forwards each message through `ingest_telegram_channel_message(...)`.
    - Discord: when `channels.discord.operation_mode=transport` and `channels.discord.staging_channel_ids` is non-empty, the gateway polls each staging channel via `DiscordTransportClient.get_channel_messages_with_retry` with a per-channel cursor and forwards new messages through `ingest_discord_channel_message(...)`.
  - Internal forwarding uses a per-process `internal_channel_ingest_token` injected via `x-carsinos-internal-ingest-token`, which `require_bearer_auth` treats as an internal service principal.
  - Listener progress emits `channel.listener.tick` WS events with `{provider, processed_messages, active}` payload.

- **Agent Mail model (gateway + MC)**:
  - Storage (source: `migrations/0001_init.sql` + `crates/carsinos-storage/src/lib.rs` tests):
    - threads, participants, messages, recipients (ack state), attachments, FTS index, and file leases.
  - Gateway endpoints: `/api/v1/agent-mail/*` + `/api/v1/agent-mail/mcp` facade (see `build_app()` route list).
  - Event stream: emits `agent_mail.*` events for thread/message/attachment/lease activities (used by Mission Control to trigger refresh).
  - UI surfaces: Mission Control `Mail` + `Rooms` tabs (`features/agentMail/*`).

- **Jobs/scheduler model** (source: `crates/carsinos-gateway/src/main.rs` + `migrations/0001_init.sql`):
  - Scheduler loop runs only when instance lock is acquired (`acquire_scheduler_instance_lock`), and uses DB leases to pick up due jobs (`jobs` + `job_runs` tables).
  - Supported schedule kinds: `interval`, `once`, `every`, `at`, `cron` (validated by gateway).
  - Job execution integrates with run engine, circuit breakers, and heartbeat mode constraints (process-level E2E tests cover scheduler behavior).

- **Memory model**:
  - Local memory: sqlite `notes` + `embeddings` tables; `carsinos-storage` supports embedding search and replacement.
  - Numquam integration: optional external memory context + writeback with degrade-mode fallback; surfaced in `/api/v1/status`, `/api/v1/jobs/status`, `/api/v1/runs/{run_id}/memory/why`, and used during run execution.

- **Security/audit model**:
  - Auth: static bearer or JWT modes; request IDs + rate limiting; role enforcement; internal ingest auth for channel listeners.
  - Audit: `security_audit_events` table + `GET /api/v1/security/audit` query API; retention job can archive/prune into `security_audit_events_archive` (see endpoints + scripts under `scripts/`).

- **Process-level E2E coverage** (source: `crates/carsinos-gateway/tests/e2e_process.rs`):
  - Covered: sessions/runs persistence across restart; WS event stream + query-token auth; approvals request/resolve; scheduler job execution + history; runtime config update paths; agent-mail threads/messages/attachments/leases + MCP facade.
  - Not observed (search-based): boards endpoints are not referenced in the process-level E2E test file (no `boards` string matches), but **are** covered by gateway crate unit tests (e.g. `board_card_create_move_and_asset_upload_round_trip`, `board_automation_rule_upsert_state_run_and_move_flow`).

## 3.1) Client ↔ Gateway endpoint usage map (connector cross-check)

### Mission Control (apps/mission-control)

- Single API wrapper: `apps/mission-control/src/lib/api.ts` (HTTP) + `apps/mission-control/src/lib/ws.ts` (WS).
- HTTP endpoints used:
  - `/api/v1/health`, `/api/v1/status`
  - `/api/v1/boards`, `/api/v1/boards/{board_id}` + card move/update/create/run + asset upload/download
  - `/api/v1/mission-control/calendar/week`, `/api/v1/mission-control/focus`
  - `/api/v1/jobs`, `/api/v1/jobs/status`, `/api/v1/jobs/{job_id}/run`, `/api/v1/jobs/{job_id}/update`
  - `/api/v1/approvals` + `/api/v1/approvals/{approval_id}/resolve`
  - `/api/v1/auth/profiles` + provider profile order endpoints
  - `/api/v1/auth/openai/oauth/{start,finish}`, `/api/v1/auth/anthropic/setup-token/ingest`
  - `/api/v1/extensions/{skills,plugins}` + plugin runtime status
  - `/api/v1/channels/runtime/{status,reconnect}`
  - `/api/v1/agent-mail/*` (threads/messages/ack/attachments/leases)
  - `/api/v1/memory/notes` (thread summary → note)
- WS endpoint used:
  - `/api/v1/ws` with `?token=` fallback (client builds `ws(s)://.../api/v1/ws?token=...`).

### egui GUI client (crates/carsinos-gui)

- Uses `ureq` + JSON parsing helpers (see `fetch_gateway_snapshots()` and action handlers in `crates/carsinos-gui/src/main.rs`).
- Snapshot + wizard endpoints used include:
  - `/api/v1/health`, `/api/v1/status`
  - `/api/v1/sessions`, `/api/v1/sessions/{session_id}/messages`, `/api/v1/sessions/{session_id}/runs`
  - `/api/v1/approvals/{approval_id}/resolve`
  - `/api/v1/auth/profiles` + `/api/v1/auth/profiles/{auth_profile_id}/state` + OpenAI OAuth + Anthropic ingest + agent profile order endpoints
  - `/api/v1/config/channels`
  - `/api/v1/config/runtime` + `/rollback` + `/secrets/upsert`
  - `/api/v1/agents`, `/api/v1/boards`, `/api/v1/jobs`, `/api/v1/memory/notes`
  - boards: `/api/v1/boards/{board_id}` + card create/move/run/update + automation upsert/state/run
  - jobs: `/api/v1/jobs/{job_id}/run`, `/api/v1/jobs/{job_id}/update`

### Scripts/harnesses (scripts/)

- `scripts/channel_soak_runner.py` (and unit tests under `scripts/tests/`) calls:
  - `/api/v1/channels/runtime/status`
  - `/api/v1/channels/{telegram,discord}/inbound`
  - `/api/v1/security/audit`
  - `/api/v1/sessions` + session messages/runs
  - `/api/v1/approvals` + `/api/v1/channels/approvals/resolve`
- `scripts/launch_mission_control_tauri_dev.command` polls:
  - `/api/v1/status`

## 3.2) E2E walkthroughs (core user journeys)

> These are intended to verify “connector points” across UI ↔ gateway ↔ storage/tools/providers/channels, with concrete file pointers for each hop.

### Journey A — Mission Control connects to gateway + starts WS stream

1. Operator enters `gateway_url` + bearer token (MC settings modal / onboarding).
2. MC persists:
   - URL → localStorage `mission_control.runtime.connection.v1`
   - token → keychain via Tauri (`set_gateway_token`) or localStorage fallback (`mission_control.runtime.token.v1`)
3. MC baseline fetch:
   - `GET /api/v1/health` → `health()` (gateway) → sqlite `ping()` gating `ok` status
   - `GET /api/v1/boards`, `GET /api/v1/agents` (baseline list rendering)
4. MC opens WS:
   - `GET /api/v1/ws?token=...` → `ws_handler()` → `handle_socket()` → initial `gateway.status` frame + subsequent broadcast events
5. UI surfaces:
   - connection dot / health chips from `healthState` + `wsState` (MC)

### Journey B — Sessions/messages/runs (gateway core loop)

1. Create session:
   - `POST /api/v1/sessions` → `create_session()` → sqlite `sessions` table
2. Add user message:
   - `POST /api/v1/sessions/{session_id}/messages` → `create_message()` → sqlite `messages` table
3. Create run:
   - `POST /api/v1/sessions/{session_id}/runs` → `create_run()` → sqlite `runs` table → `execute_run_with_lane_control()` → `execute_run()`
4. Run execution emits WS events:
   - `run.status` (`running` → `succeeded|failed`)
   - `run.delta` (tool summaries + provider deltas)

### Journey C — Tool execution + approval gating + resume

1. User message contains tool lines (one per line), e.g. `tool.exec echo hi`.
2. Run execution parses tool calls from latest user message (`parse_tool_requests_from_input`).
3. For approval-required tools:
   - gateway creates `tool_calls` + `approvals` records, emits `approval.requested`, and blocks run until resolved.
4. Operator resolves:
   - `POST /api/v1/approvals/{approval_id}/resolve` → `resolve_approval()`
5. Resume run:
   - `POST /api/v1/runs/{run_id}/resume` → `resume_run()` continues tool execution and provider completion.

### Journey D — Boards: create/move/run card from Mission Control

1. Boards tab lists/selects board:
   - `GET /api/v1/boards` + `GET /api/v1/boards/{board_id}`
2. Create card:
   - `POST /api/v1/boards/{board_id}/cards/create` → `create_board_card()` → sqlite `board_cards`
   - emits `board.card.created` (MC refreshes board)
3. Move card:
   - `POST /api/v1/boards/{board_id}/cards/{card_id}/move` → `move_board_card()` → emits `board.card.moved` (MC updates card position optimistically)
4. Run card:
   - `POST /api/v1/boards/{board_id}/cards/{card_id}/run` → `run_board_card()`:
     - creates/reuses a session keyed to board/card/agent
     - writes a user message prompt derived from card fields
     - creates + executes run
     - updates `board_cards.linked_session_id` + `latest_run_id`
   - emits `board.card.run` (MC updates `latest_run_id`)

### Journey E — Agent Mail: thread + message + ack + attachment + file lease

1. Create/list thread:
   - `GET/POST /api/v1/agent-mail/threads` → thread stored in sqlite `agent_mail_threads` (+ participants)
2. Send message (+ optional attachments):
   - `POST /api/v1/agent-mail/threads/{thread_id}/messages` → inserts into `agent_mail_messages` + `agent_mail_message_recipients`
   - `POST /api/v1/agent-mail/messages/{message_id}/attachments/upload` → writes `<state_dir>/attachments/...` + `agent_mail_attachments`
3. Ack:
   - `POST /api/v1/agent-mail/messages/{message_id}/ack` updates recipient `acked_at`
4. File leases:
   - `GET/POST /api/v1/agent-mail/leases` + `POST /api/v1/agent-mail/leases/{lease_id}/release` → sqlite `agent_mail_file_leases`
5. MC refresh trigger:
   - WS events prefixed `agent_mail.*` cause MC to refresh mail read models.

### Journey F — Channel inbound message → routing → run → outbound transport reply

1. Channel transport ingestion (transport mode only):
   - In-process listeners poll Telegram long-poll updates or Discord staging channels.
2. Listener forwards to ingest handlers with internal auth header:
   - `ingest_telegram_channel_message(...)` / `ingest_discord_channel_message(...)`
3. Ingest handler applies allowlist/mention gating via channel adapter crate (`route_message`).
4. On accept:
   - creates/reuses a session by stable session key (per DM/group)
   - creates a run (optional `run_immediately`) and emits WS events
5. If transport reply enabled:
   - uses transport client to send message chunks back to channel.

### Journey G — Extensions: plugin hooks + plugin tools (daemon/subprocess)

1. Gateway loads plugin manifests at startup (`PluginRegistry::load_from_dirs`).
2. Hook points emitted during runs/tool execution call `emit_extension_hook_point()`:
   - invokes plugin hook runner(s) and records internal security audit outcomes.
3. Plugin tool calls:
   - user message includes `tool.<plugin_tool>`; parsed via `ToolRegistry::parse_line_with_plugins`
   - executed via plugin runner; daemon-mode requires runtime allowlist (`extensions.plugin_daemon_allowlist`).

## 4) Config / secrets / runtime state map (to verify)

- **Environment variables inventory (grep-based)** (source: `rg -o "CARSINOS_[A-Z0-9_]+" -S crates apps scripts | sort -u`):
  - gateway/bootstrap: `CARSINOS_GATEWAY_BIND`, `CARSINOS_GATEWAY_TOKEN`, `CARSINOS_STATE_DIR`, `CARSINOS_PUBLIC_BIND_ALLOWED`
  - auth:
    - mode select: `CARSINOS_AUTH_MODE`
    - JWT: `CARSINOS_AUTH_JWT_ISSUER`, `CARSINOS_AUTH_JWT_AUDIENCE`, `CARSINOS_AUTH_JWT_HS256_SECRET`, `CARSINOS_AUTH_JWT_CLOCK_SKEW_SECONDS`, `CARSINOS_AUTH_JWT_MAX_TOKEN_AGE_SECONDS`, `CARSINOS_AUTH_JWT_REPLAY_PROTECTION_ENABLED`, `CARSINOS_AUTH_JWT_REVOKED_JTIS`
    - tooling/testing scripts: `CARSINOS_AUTH_TOKEN`, `CARSINOS_OPERATOR_PEER_ID`
  - edge/proxy trust: `CARSINOS_TRUST_PROXY_HEADERS`, `CARSINOS_TRUSTED_PROXY_ALLOWLIST`, `CARSINOS_EDGE_TLS_TERMINATED`, `CARSINOS_TRUST_LOCK_OWNER`
  - rate limits: `CARSINOS_RATE_LIMIT_ENABLED`, `CARSINOS_RATE_LIMIT_WINDOW_SECONDS`, `CARSINOS_RATE_LIMIT_PER_IP`, `CARSINOS_RATE_LIMIT_PER_PRINCIPAL`, `CARSINOS_RATE_LIMIT_RUN_ENDPOINT`, `CARSINOS_RATE_LIMIT_APPROVAL_ENDPOINT`
  - logging: `CARSINOS_LOG_FILTER`, `CARSINOS_LOG_FORMAT`, `CARSINOS_LOG_STDOUT`, `CARSINOS_LOG_FILE`, `CARSINOS_LOG_FILE_PREFIX`
  - tools sandbox/runtime: `CARSINOS_TOOL_ALLOWED_ROOTS`, `CARSINOS_TOOL_ALLOWED_BINARIES`, `CARSINOS_TOOL_NETWORK_POLICY`, `CARSINOS_TOOL_NETWORK_ALLOWLIST`, `CARSINOS_TOOL_MAX_READ_BYTES`, `CARSINOS_TOOL_MAX_OUTPUT_CHARS`, `CARSINOS_TOOL_MAX_CONCURRENCY`, `CARSINOS_TOOL_DEF_EXEC_TIMEOUT_MS`, `CARSINOS_TOOL_DEF_PROCESS_TIMEOUT_MS`, `CARSINOS_TOOL_CHANNEL_ALLOWED_PROVIDERS`, `CARSINOS_WEB_SEARCH_BASE_URL`
  - extensions: `CARSINOS_PLUGIN_MANIFEST_DIRS`, `CARSINOS_EXTENSION_ALLOWED_PLUGIN_IDS`, `CARSINOS_EXTENSION_RESERVED_SKILL_IDS`, `CARSINOS_EXTENSION_RESERVED_SKILL_PREFIXES`, `CARSINOS_SKILL_DIRS`, `CARSINOS_SKILL_INJECTION_MAX_CHARS`, `CARSINOS_SKILL_INJECTION_MAX_CHARS_PER_SKILL`, `CARSINOS_SKILL_INJECTION_MAX_SKILLS`, `CARSINOS_PLUGIN_ID`, `CARSINOS_PLUGIN_KIND`
  - providers/auth bootstrap: `CARSINOS_OPENAI_OAUTH_CLIENT_ID`, `CARSINOS_OPENAI_OAUTH_REDIRECT_URI`, `CARSINOS_OPENAI_OAUTH_AUTHORIZE_URL`, `CARSINOS_OPENAI_OAUTH_TOKEN_URL`, `CARSINOS_OPENAI_OAUTH_SCOPE`, `CARSINOS_OPENAI_API_BASE_URL`
  - Numquam integration: `CARSINOS_NUMQUAM_ENABLED`, `CARSINOS_NUMQUAM_TRANSPORT`, `CARSINOS_NUMQUAM_BASE_URL`, `CARSINOS_NUMQUAM_MCP_URL`, `CARSINOS_NUMQUAM_TIMEOUT_MS`, `CARSINOS_NUMQUAM_TOKEN`, `CARSINOS_NUMQUAM_PRINCIPAL_ID`, `CARSINOS_NUMQUAM_PRINCIPAL_NAME`
  - agent-mail limits: `CARSINOS_AGENT_MAIL_ALLOWED_MIME`, `CARSINOS_AGENT_MAIL_ATTACHMENT_MAX_BYTES`, `CARSINOS_AGENT_MAIL_MCP_DEFAULT_LEASE_TTL_MS`
  - local memory/embeddings: `CARSINOS_LOCAL_MEMORY_ENABLED`, `CARSINOS_LOCAL_MEMORY_MAX_CHARS`, `CARSINOS_LOCAL_MEMORY_MAX_CANDIDATES`, `CARSINOS_LOCAL_MEMORY_TOP_K`
  - approvals: `CARSINOS_AUTO_APPROVE_TOOLS`
  - security audit retention: `CARSINOS_SECURITY_AUDIT_HOT_RETENTION_DAYS`
  - GUI-specific: `CARSINOS_GATEWAY_URL`, `CARSINOS_GATEWAY_BIN`, `CARSINOS_GUI_AUTO_LAUNCH_GATEWAY`
  - channel soak harness: `CARSINOS_BASE_URL`, `CARSINOS_DISCORD_CHANNEL_ID`, `CARSINOS_DISCORD_AUTHOR_ID`, `CARSINOS_TELEGRAM_CHAT_ID`, `CARSINOS_TELEGRAM_USER_ID`

- **Mission Control runtime settings keys** (source: `apps/mission-control/src/lib/runtime.ts`):
  - `mission_control.runtime.connection.v1` (localStorage JSON containing `gateway_url`)
  - `mission_control.runtime.token.v1` (localStorage fallback token for non-Tauri runtime)

- **Mission Control keychain storage (Tauri)** (source: `apps/mission-control/src-tauri/src/lib.rs`):
  - service: `carsinos.mission-control`
  - username: `gateway-token`

- **Gateway secret store** (source: `crates/carsinos-gateway/src/main.rs` → `SecretStore`):
  - backend: keychain by default (`CARSINOS_SECRET_STORE=keychain`), or in-memory for tests (`CARSINOS_SECRET_STORE=memory`).
  - service name: `CARSINOS_SECRET_SERVICE` (default `carsinos`).
  - secret ref format (general): `secret://<key>` where `<key>` is stored in the secret backend.
  - runtime-scoped secret helper:
    - scope → key: `runtime_secret_key_from_scope(scope)` → `runtime.<scope>` with `/` replaced by `.`
    - key → ref: `secret://runtime.<scope>`
    - enforced by runtime config validation: secret refs must start with `secret://` for secret-ref fields.
  - runtime secret mutation endpoints:
    - `POST /api/v1/config/runtime/secrets/upsert` (stores `secret_value` under derived key; can delete `previous_secret_ref` if provided)
    - `POST /api/v1/config/runtime/secrets/delete`
    - both trigger `state.channel_runtime.reconcile()` (channel runtime refresh).

- **Auth profile credential hydration (providers connector)** (source: `crates/carsinos-gateway/src/main.rs` → `hydrate_auth_profile_credentials`):
  - `auth_profiles.credentials_json` (sqlite) stores **metadata**, typically including a `secret_ref` string key.
  - The referenced secret payload is stored in `SecretStore` under that key (e.g. `auth.openai.oauth.<uuid>`, `auth.anthropic.setup-token.<uuid>`), and is merged into metadata at runtime (selected keys only) before sending to provider adapters (`carsinos-providers`).
  - This allows DB records to avoid storing plaintext tokens while still materializing provider credentials for outbound HTTPS calls.
  - Note: runtime-config secret refs use `secret://...` format, while auth-profile `secret_ref` values are stored as raw secret-store keys (no `secret://` prefix) in current implementation.

- **Numquam integration configuration (memory connector)** (source: `crates/carsinos-gateway/src/main.rs` + `crates/carsinos-protocol/src/lib.rs`):
  - Env compatibility mode: `NumquamClient::from_env()` is enabled when `CARSINOS_NUMQUAM_ENABLED=true` or `CARSINOS_NUMQUAM_BASE_URL` is set (defaults base URL to `http://127.0.0.1:7340`, transport `dual`).
  - Runtime-config mode: `NumquamClient::from_runtime_config(runtime_config.memory.numquam, secret_store)` is used when `memory.numquam.enabled=true`; supports `transport=http|mcp|dual` and per-operation timeouts/intervals.
  - Token sourcing:
    - runtime-config: `memory.numquam.token_secret_ref` uses `secret://...` format and is resolved via `SecretStore` (`secret_key_from_secret_ref` + `get_raw`).
    - env fallback: `CARSINOS_NUMQUAM_TOKEN`.
  - Handshake loop: `numquam_handshake_loop` periodically calls `capabilities.get` + `health.get`, updates `state.numquam_runtime_status`, and records circuit breaker + audit events; degraded status triggers run-time fallback behavior during memory context/writeback stages.

- **Runtime channel transport requirements (selected)** (source: `crates/carsinos-gateway/src/main.rs`):
  - Discord:
    - `channels.discord.operation_mode=transport` requires non-empty `channels.discord.bot_token_secret_ref` and resolvable secret value in `SecretStore` (see `build_discord_transport_client()`).
  - Telegram:
    - `channels.telegram.operation_mode=transport` requires non-empty `channels.telegram.bot_token_secret_ref` and resolvable secret value in `SecretStore` (see `build_telegram_transport_client()`).
    - additionally, when `operation_mode=transport` and `channels.telegram.webhook_mode=webhook`, `channels.telegram.webhook_url` is required (see `TelegramRuntimeAdapter::resolve_runtime_state()`).

- **Gateway state directory layout** (source: `crates/carsinos-storage/src/lib.rs` → `AppPaths::from_root` + gateway usage):
  - root: `CARSINOS_STATE_DIR` (or default via `directories` in `carsinos-core::GatewayConfig`)
  - SQLite DB: `<state_dir>/carsinos.db`
  - attachments root: `<state_dir>/attachments/`
  - logs: `<state_dir>/logs/`
  - boards assets: `<state_dir>/attachments/board_cards/<card_id>/...` (gateway writes here in `upload_board_card_asset`)

- **Logging + request IDs** (source: `crates/carsinos-gateway/src/main.rs`):
  - tracing init: `init_tracing()` uses `CARSINOS_LOG_FILTER`, `CARSINOS_LOG_FORMAT`, `CARSINOS_LOG_STDOUT`, `CARSINOS_LOG_FILE`, `CARSINOS_LOG_FILE_PREFIX`; file logs roll daily into `<state_dir>/logs/`.
  - HTTP request IDs: `PropagateRequestIdLayer::x_request_id()` + `SetRequestIdLayer::x_request_id(MakeRequestUuid)` ensures a stable `x-request-id` on responses and includes it in request spans.

- **Default extension directories** (source: `crates/carsinos-gateway/src/main.rs`):
  - skills: `CARSINOS_SKILL_DIRS` (CSV) or default `<state_dir>/skills`
  - plugin manifests: `CARSINOS_PLUGIN_MANIFEST_DIRS` (CSV) or default `<state_dir>/plugins/active` (fallback to `<state_dir>/plugins` if it contains manifests)

## 5) Validation commands (to run / evidence to capture)

> Capture outputs/summaries here (don’t paste huge logs).

- Rust:
  - `cargo test --workspace --locked` ✅ PASS (workspace tests + gateway process E2E/benchmarks)
  - `cargo clippy --workspace --all-targets -- -D warnings`
- Mission Control:
  - `cd apps/mission-control && npm run typecheck` ✅ PASS
  - `cd apps/mission-control && npm run lint` ✅ PASS
  - `cd apps/mission-control && npm run test` (no script present in `package.json`)
  - `cd apps/mission-control && npm run build` ✅ PASS (built in ~1.03s; JS gzip ~111 kB)
  - `cd apps/mission-control/src-tauri && cargo check` ✅ PASS

## 6) Findings (append-only)

> Format:
> - **ID**:
> - **Severity**: P0/P1/P2/P3
> - **Summary**:
> - **Evidence**: file(s)/line(s), command(s), screenshots, logs
> - **Impact**:
> - **Suggested fix (not implemented)**:

- **F-001**
  - **Severity**: P2
  - **Summary**: Mission Control maintains a hand-written TS API/type surface (`apps/mission-control/src/types.ts`) that only partially mirrors `crates/carsinos-protocol/src/lib.rs`, creating ongoing drift risk for UI↔gateway connector points (e.g., TS `HealthResponse`/`StatusResponse` omit fields present in Rust protocol).
  - **Evidence**: `apps/mission-control/src/types.ts` vs `crates/carsinos-protocol/src/lib.rs` (`HealthResponse`, `StatusResponse`, `JobStatusResponse`, etc).
  - **Impact**: Contract changes in gateway/protocol can silently break UI assumptions; missing/extra fields are currently tolerated via optional typing but can mask true incompatibilities.
  - **Suggested fix (not implemented)**: Generate TS types from protocol schemas (or OpenAPI/JSON schema) and validate at build-time; alternatively, narrow runtime parsing to explicit “used fields” with runtime assertions.

- **F-002**
  - **Severity**: P3
  - **Summary**: Mission Control has no automated test script configured (no `npm run test`), so UI regressions rely on typecheck/lint/build + manual validation.
  - **Evidence**: `apps/mission-control/package.json` scripts list does not include `test`.
  - **Impact**: Lower confidence in UI feature behavior, especially for e2e connector points (boards drag/drop, agent mail flows, onboarding).
  - **Suggested fix (not implemented)**: Add at least smoke-level component/integration tests (e.g., Playwright/Vitest) for critical flows and API contract parsing.

- **F-003**
  - **Severity**: P3
  - **Summary**: Gateway process-level E2E tests cover many critical surfaces but do not exercise boards endpoints at the process layer (boards are covered by gateway crate unit tests instead).
  - **Evidence**: `crates/carsinos-gateway/tests/e2e_process.rs` has no `boards` references; gateway unit tests include `board_card_*` and board automation coverage (see `cargo test` output / `crates/carsinos-gateway/src/main.rs` tests module).
  - **Impact**: Boards workflows may be less protected against “real process” regressions (restart persistence, auth headers, request ID propagation) compared to sessions/jobs/agent-mail.
  - **Suggested fix (not implemented)**: Add one minimal process-level board roundtrip test (list boards, get board, create+move card, upload asset).

- **F-004**
  - **Severity**: P3
  - **Summary**: `create_auth_profile` rejects unsupported providers with an error string that lists an outdated subset of supported providers.
  - **Evidence**: `crates/carsinos-gateway/src/main.rs`: `provider_supported()` allows `openrouter|ollama|vllm`, but the `create_auth_profile` error message says `"expected: mock, openai, anthropic, unconfigured"`.
  - **Impact**: Operator/dev confusion during manual API use; harder debugging when adding new provider IDs.
  - **Suggested fix (not implemented)**: Update error message to match `provider_supported()` (or build it dynamically from a registry).

- **F-005**
  - **Severity**: P3
  - **Summary**: Channel soak runner defaults to a gateway base URL port (`7341`) that differs from the gateway’s default bind (`18789`), which can cause “out of the box” confusion when running the soak harness without env overrides.
  - **Evidence**: `scripts/channel_soak_runner.py` defaults `--base-url` to `http://127.0.0.1:7341`; `carsinos-core::GatewayConfig` default bind is `127.0.0.1:18789`.
  - **Impact**: First-run soak harness attempts may fail unless operators set `CARSINOS_BASE_URL`/`--base-url`.
  - **Suggested fix (not implemented)**: Align default port (or document why a separate port is expected).
