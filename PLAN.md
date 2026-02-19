## carsinOS (Rust) Ground-Up Build Plan

### Summary
Build a new, Rust-native “AI gateway + agent” inspired by OpenClaw, optimized for speed/stability and clean modularity, not feature parity. MVP is macOS-local, single-user, with a Rust GUI (egui), a Rust gateway daemon, Telegram + Discord channels, tool-calling (exec/fs/web), Brave Search integration, SQLite as source-of-truth, and “OAuth where supported + API keys fallback” for model providers. Exec/tool approvals must work in both GUI and chat channels.

### Non-Goals (v1)
- Reusing or porting OpenClaw code directly (we only borrow ideas and protocol shapes).
- Mobile “node” pairing/canvas/screen/camera remoting.
- Web frontend stack (no React/Next/etc).
- Multi-tenant SaaS, plugin marketplace, or dynamic untrusted extension loading.
- Perfect formatting parity across Discord/Telegram/GUI.

---

## 1) Architecture (Processes + Data Flow)

### Processes
- `carsinos-gateway` (daemon): owns channels, agent loop, tools, storage, providers, HTTP+WS API.
- `carsinos-gui` (egui desktop app): local operator UI; connects to gateway via token-auth; can also spawn/stop gateway in dev.
- `carsinos` (CLI): starts gateway, runs auth wizards, diagnostics, and headless operation.

### High-Level Diagram
```mermaid
flowchart LR
  GUI["carsinos-gui (egui)"] -->|HTTP (commands)| GW["carsinos-gateway"]
  GUI <-->|WS (events/stream)| GW

  DIS["Discord Bot (serenity)"] -->|inbound msg| GW
  TEL["Telegram Bot (teloxide)"] -->|inbound msg| GW

  GW --> DB["SQLite (carsinos.db)"]
  GW --> FS["Attachments + workspace FS"]
  GW --> LLM["Providers: OpenAI / Anthropic / Local"]
  GW --> SEARCH["Brave Search API"]
```

### Core Invariants
- SQLite is the authoritative state store for sessions/messages/runs/approvals/config metadata.
- One active agent run per session at a time (serialized per-session lane).
- Tools are gated by policy + approvals; channel message senders never get implicit tool power.

---

## 2) Repo/Workspace Layout (Rust)

Create a new workspace folder at:
- `/Users/domusanimae/Documents/openclaw replacement/carsinos`

Workspace layout:
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/Cargo.toml` (workspace)
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-core`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-protocol`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-storage`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-tools`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-providers`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-channels-discord`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-channels-telegram`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-gateway` (binary)
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-gui` (binary)
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/crates/carsinos-cli` (binary)
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/migrations` (SQL migrations)
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/README.md`

Crate responsibilities:
- `carsinos-core`: agent loop, domain types, routing, approvals state machine, tool registry.
- `carsinos-protocol`: HTTP/WS request/response/event types, JSON schema generation via `schemars`.
- `carsinos-storage`: SQLite access layer + migrations + repositories.
- `carsinos-tools`: exec/fs/web_fetch/web_search tools + safety limits.
- `carsinos-providers`: OpenAI/Anthropic/Local model adapters + embeddings adapters + auth flows.
- channel crates: adapters + formatting + allowlist/mention gating.
- `carsinos-gateway`: `axum` HTTP+WS server + background tasks + composition root.
- `carsinos-gui`: egui UI + markdown rendering + local operator approvals + onboarding.
- `carsinos-cli`: `clap` commands for gateway/auth/status/debug.

---

## 3) Storage: Paths, Secrets, Permissions

### App Directories (macOS)
Use `directories` crate to resolve:
- Config dir: `~/Library/Application Support/carsinos/`
- Database: `~/Library/Application Support/carsinos/carsinos.db`
- Attachments: `~/Library/Application Support/carsinos/attachments/`
- Logs: `~/Library/Application Support/carsinos/logs/`

Enforce permissions at runtime:
- Ensure app dir is `0700`
- Ensure DB/config files are `0600`

### Secrets
Store secrets in macOS Keychain via `keyring` crate:
- Gateway API token
- Discord bot token
- Telegram bot token
- Brave Search API key
- OpenAI API key
- Anthropic API key
- OAuth refresh tokens (OpenAI Codex OAuth)

Config file references secrets by logical name (not plaintext).

---

## 4) SQLite Schema (Decision-Complete)

Create migrations in `/Users/domusanimae/Documents/openclaw replacement/carsinos/migrations/0001_init.sql`.

Tables (minimum viable; extend later):

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_kv (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
  agent_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  workspace_root TEXT NOT NULL,
  model_provider TEXT NOT NULL,
  model_id TEXT NOT NULL,
  tool_profile TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS routing_rules (
  rule_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  channel TEXT NOT NULL,            -- "discord" | "telegram" | "gui" | "cli"
  channel_account TEXT,             -- optional (bot/account id)
  peer_id TEXT,                     -- sender id for DMs, optional
  conversation_id TEXT,             -- channel/thread/chat id, optional
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  session_scope TEXT NOT NULL,      -- "main" | "per-peer" | "per-conversation"
  require_mention INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY,
  session_key TEXT NOT NULL UNIQUE, -- derived key for routing, stable string
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  title TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER
);

CREATE TABLE IF NOT EXISTS messages (
  message_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(session_id),
  source_channel TEXT NOT NULL,      -- "discord" | "telegram" | "gui" | "cli" | "system"
  source_peer_id TEXT,               -- user id on platform
  source_message_id TEXT,            -- platform message id
  role TEXT NOT NULL,                -- "user" | "assistant" | "tool" | "system"
  content_text TEXT NOT NULL,
  content_format TEXT NOT NULL,      -- "markdown" | "plain"
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session_time ON messages(session_id, created_at);

CREATE TABLE IF NOT EXISTS attachments (
  attachment_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES messages(message_id),
  kind TEXT NOT NULL,                -- "image" | "file"
  mime TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  local_path TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  run_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(session_id),
  status TEXT NOT NULL,              -- "queued"|"running"|"succeeded"|"failed"|"canceled"
  model_provider TEXT NOT NULL,
  model_id TEXT NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  error_text TEXT,
  usage_json TEXT,                   -- provider usage blob
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_session_time ON runs(session_id, created_at);

CREATE TABLE IF NOT EXISTS tool_calls (
  tool_call_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(run_id),
  tool_name TEXT NOT NULL,           -- "exec"|"fs.read"|"web.search"|...
  args_json TEXT NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  status TEXT NOT NULL,              -- "pending"|"running"|"succeeded"|"failed"|"canceled"
  result_json TEXT,                  -- truncated + redacted
  error_text TEXT
);

CREATE TABLE IF NOT EXISTS approvals (
  approval_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(run_id),
  tool_call_id TEXT NOT NULL REFERENCES tool_calls(tool_call_id),
  kind TEXT NOT NULL,                -- "exec"|"fs.write"|"web.fetch" (extensible)
  status TEXT NOT NULL,              -- "requested"|"approved"|"denied"|"expired"
  request_summary TEXT NOT NULL,
  request_json TEXT NOT NULL,
  requested_at INTEGER NOT NULL,
  decided_at INTEGER,
  decided_via TEXT,                  -- "gui"|"discord"|"telegram"
  decided_by_peer_id TEXT
);

CREATE TABLE IF NOT EXISTS notes (
  note_id TEXT PRIMARY KEY,
  title TEXT,
  body TEXT NOT NULL,
  tags_json TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS embeddings (
  embedding_id TEXT PRIMARY KEY,
  source_kind TEXT NOT NULL,         -- "message"|"note"
  source_id TEXT NOT NULL,           -- message_id or note_id
  chunk_index INTEGER NOT NULL,
  model TEXT NOT NULL,               -- embedding model id
  dims INTEGER NOT NULL,
  vec BLOB NOT NULL,                 -- f32 LE array
  text TEXT NOT NULL,                -- chunk text (for display + fallback)
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_embeddings_source ON embeddings(source_kind, source_id);
```

Vector retrieval v1:
- Load candidate vectors (by scope: session + notes, or notes only; configurable).
- Compute cosine similarity in Rust; return top K.
- Hard caps: max candidates per query, max chunks injected, max chars injected.

---

## 5) Gateway API (HTTP + WebSocket)

### Auth
- All HTTP/WS require `Authorization: Bearer <gateway_token>`.
- Default bind: `127.0.0.1:18789`.
- Remote enablement is opt-in by config (bind to LAN/Tailscale).

### HTTP Endpoints (v1)
Base: `/api/v1`

- `GET /health` -> `{ ok: true, version, uptime_ms }`
- `GET /status` -> gateway state, channel connectivity, provider status summary
- `GET /config` -> sanitized config (no secrets)
- `PATCH /config` -> update config fields (writes to config file and app_kv)

Sessions:
- `GET /sessions?limit=&cursor=` -> list
- `POST /sessions` -> create `{ agent_id, title? }`
- `GET /sessions/{session_id}` -> details
- `POST /sessions/{session_id}/messages` -> append message `{ role=user, content_text, attachments? }`
- `POST /sessions/{session_id}/runs` -> start run `{ input_message_id?, tool_profile_override?, model_override? }`
- `POST /runs/{run_id}/cancel` -> cancel

Approvals:
- `GET /approvals?status=requested` -> list pending
- `POST /approvals/{approval_id}/resolve` -> `{ decision: "approve"|"deny" }`

Providers/Auth:
- `GET /providers` -> configured providers + logged-in profiles
- `POST /auth/openai-codex/start` -> returns `{ authorize_url, state, pkce_verifier_id }`
- `POST /auth/openai-codex/finish` -> `{ state, code | redirect_url }`
- `POST /auth/anthropic/setup-token` -> `{ token }`
- `POST /auth/openai/api-key` -> `{ key }`
- `POST /auth/anthropic/api-key` -> `{ key }`

Channels:
- `GET /channels` -> status
- `POST /channels/reload` -> reconnect bots (safe restart)

### WebSocket: `/api/v1/ws`
Server->client events only (commands via HTTP). Frame format:
```json
{ "v": 1, "type": "event", "event": "run.delta", "seq": 123, "data": { ... } }
```

Event set:
- `gateway.status` (periodic)
- `session.updated` (title/updated_at)
- `message.created` (new message)
- `run.created`
- `run.status` (queued/running/finished)
- `run.delta` (assistant streaming text delta)
- `run.tool_call` (tool start + args summary)
- `run.tool_result` (tool end + result summary)
- `approval.requested`
- `approval.resolved`
- `channel.inbound` (for debugging/monitoring)
- `channel.outbound` (for debugging/monitoring)

Backpressure policy:
- WS keeps a bounded per-connection queue; on overflow, disconnect with a clear reason; GUI auto-reconnect and refreshes `/sessions`.

---

## 6) Agent Loop (Tool-Calling, Bounded, Stable)

### Internal Run State Machine
For each run:
1. Acquire session lane semaphore.
2. Build context:
   - system prompt base + policy + tool list
   - last N messages (configurable)
   - retrieved memory chunks (notes + session chunks)
   - attachment descriptors (images)
3. Call provider with streaming enabled.
4. While streaming:
   - emit `run.delta` for assistant text
   - if provider emits a tool call, pause assistant stream, record `tool_calls` row, emit `run.tool_call`
5. Tool execution phase:
   - validate args against tool schema
   - run policy checks
   - if approval required, create `approvals` row and emit `approval.requested`
   - wait for approval resolution (GUI or channel)
   - execute tool
   - truncate/redact outputs
   - persist `tool_calls.result_json`, emit `run.tool_result`
6. Feed tool results back into provider and continue loop.
7. Stop conditions:
   - provider produces a final assistant message
   - max tool rounds reached (default 8)
   - run canceled
   - timeout (default 10 minutes)
8. Persist `runs` final status + usage.

### Cancellation
- `POST /runs/{run_id}/cancel` triggers a `CancellationToken`.
- Tool runners must periodically check cancellation and exit cleanly.

### Safety Limits (hard)
- Max tool rounds per run: 8
- Max exec output captured: 200k chars combined stdout+stderr (configurable)
- Max web fetch extracted text: 80k chars
- Max memory injected: 24k chars
- Attachment max bytes accepted: 10MB (images), configurable
- Message length limits for channels:
  - Discord: chunk to <= 1900 chars per message with codeblock-aware chunking
  - Telegram: chunk to safe limit; keep formatting minimal if needed

---

## 7) Tool Subsystem (exec / fs / web_fetch / web_search)

### Tool Registry
- `carsinos-tools` defines tool schemas and implementations.
- `carsinos-core` owns:
  - tool policy evaluation
  - approval decisions
  - tool result truncation/redaction

### Tools (v1)
- `exec`:
  - args: `command`, `workdir?`, `env?`, `timeout_sec?`, `background_ms?`, `pty?`
  - returns: `status`, `exit_code?`, `stdout_tail`, `stderr_tail`, `process_id?`
- `process` (paired with exec for background):
  - actions: `list`, `poll`, `kill`, `write`, `log`
- `fs.read`:
  - args: `path`, `max_bytes?`
- `fs.write`:
  - args: `path`, `content`, `mode: "create"|"overwrite"|"append"`
  - always requires approval unless explicitly disabled
- `fs.list`:
  - args: `path`, `depth?`
- `web.search`:
  - args: `query`, `count?`
  - provider: Brave Search API
- `web.fetch`:
  - args: `url`
  - returns extracted text + title + canonical url

### Approval Model (Both GUI + Channels)
- Default: approvals required for `exec` and `fs.write` and optionally `web.fetch`.
- When an approval is requested:
  - GUI shows modal with full details.
  - Discord/Telegram receive an interactive approval message (buttons/inline keyboard).
- Only allowlisted operator identities can approve/deny.
- First valid decision wins; others become no-ops and get an “already resolved” response.

---

## 8) Providers + Auth (OAuth Where Supported)

### Provider Adapters
Implement in `carsinos-providers`:
- OpenAI API (API key)
- Anthropic API (API key)
- OpenAI Codex (ChatGPT OAuth) via PKCE (as supported; modeled after OpenClaw docs at `/Users/domusanimae/Documents/openclaw replacement/openclaw/docs/concepts/oauth.md`)
- Local OpenAI-compatible endpoint (base_url + api_key optional)

### Unified Internal Interfaces
- `ModelClient` trait:
  - `stream_chat(request) -> Stream<ModelEvent>`
- `EmbeddingClient` trait:
  - `embed(texts) -> Vec<Vec<f32>>`

### OAuth UX (PKCE)
Ports and fallback:
- Default callback listener: `http://127.0.0.1:1455/auth/callback`
- If bind fails or user is remote/headless: allow paste of redirect URL.

Storage:
- Access tokens + refresh tokens stored in Keychain.
- Minimal metadata in SQLite or config (profile id, expires_at, provider kind).

Anthropic setup-token:
- GUI/CLI accepts paste.
- Store in Keychain.
- Validate by calling a lightweight endpoint (provider-specific).

---

## 9) Channels (Telegram + Discord)

### Common Channel Policy (Safe Defaults)
- DM policy default: allowlist-only.
- Group policy default: allowlist-only + require mention.
- Commands/approvals only for allowlisted operator IDs.

Store allowlists in config (non-secret), editable in GUI:
- Telegram allowlisted user ids (integers)
- Discord allowlisted user ids (snowflakes as strings)

### Telegram (teloxide)
- Mode: long polling (webhooks later)
- Message mapping:
  - session_scope "per-conversation": key = `telegram:<chat_id>` (groups)
  - session_scope "per-peer": key = `telegram:dm:<user_id>`
- Mention gating:
  - require the bot username mention or reply-to-bot message.
- Formatting:
  - Convert internal Markdown to Telegram HTML subset (best-effort).
  - If conversion fails, fall back to plain text + codeblocks.

Approvals:
- Inline keyboard with Approve/Deny.
- Callback handler validates operator id allowlist.

### Discord (serenity)
- Support:
  - DMs and guild channels
  - Threads (map to conversation_id = thread id)
- Mention gating:
  - require `@bot` mention in guild channels by default
- Formatting:
  - Discord Markdown is close to internal Markdown; preserve code fences.
  - Chunk responses to safe size.
- Approvals:
  - Use message components (buttons).
  - Validate operator id allowlist.

---

## 10) GUI (egui) and Markdown Rendering

### GUI Screens (v1)
- Status: gateway running, channels connected, providers logged in
- Sessions: list + search
- Chat: message timeline + streaming deltas
- Approvals: queue with details + approve/deny
- Providers/Auth: API keys + OAuth start/finish + profile selection
- Channels: enable/disable + tokens status + allowlists + mention gating
- Tools: tool policy toggles, exec defaults, safety limits
- Memory: notes editor + memory on/off + embedding provider

### Markdown Rendering
- Use `pulldown-cmark` parsing.
- Render in egui using a CommonMark renderer crate (or a small custom renderer).
- Supported in v1:
  - headings (render as larger text)
  - emphasis/strong
  - inline code
  - fenced code blocks (monospace, copy button)
  - links (click opens system browser)
  - blockquotes
  - lists (basic)
- Not required in v1:
  - tables (defer)
  - inline images in markdown (attachments shown separately)

---

## 11) Performance/High-Impact Areas (What Matters)
- Tool output and web fetch text must be aggressively truncated and structured. This is the single biggest stability lever.
- Avoid blocking the async runtime:
  - exec and heavy file IO in `spawn_blocking`
  - embeddings batch + cache results
- Vector memory retrieval:
  - start with brute-force cosine over bounded candidate set (fast enough for MVP)
  - add optional in-memory HNSW index later if embeddings count grows

---

## 12) Feature Menu With Performance Impact (x/10) + Recommendation
Perf score is runtime cost risk (CPU/memory/latency), not dev effort.

- Core chat + sessions (1/10): MVP.
- Streaming deltas (1/10): MVP (GUI), optional for channels (send final only).
- Discord + Telegram adapters (2/10): MVP.
- Approvals (GUI + channel) (2/10): MVP (security-critical).
- Exec + background process mgmt (5/10): MVP, but default “approval required”.
- Filesystem read/write tools (3/10): MVP; `fs.write` approval required.
- Web search (Brave) (3/10): MVP.
- Web fetch + extraction (4/10): MVP but strict truncation; optional approval toggle.
- Multi-agent routing rules (2/10): MVP (simple rules only).
- OAuth flows (OpenAI Codex PKCE) (2/10): Phase 1-2 (high UX value).
- Anthropic setup-token flow (1/10): Phase 1-2.
- Vector memory (embeddings + retrieval) (6/10): Phase 2 (bounded brute-force first).
- HNSW indexing (4/10): Phase 3 (only if needed).
- Compaction/summarization (4/10): Phase 3 (nice-to-have for long sessions).
- Plugin system (7/10): Not in v1; revisit after core is stable.
- Multi-tenancy/SaaS (9/10): Explicitly out of scope until v2+.

---

## 13) Milestones (Order + Acceptance Criteria)

### Milestone 0: Skeleton + Health
- Gateway starts, creates dirs, runs migrations.
- `GET /api/v1/health` works with token auth.

Acceptance:
- `carsinos gateway` runs; GUI connects; status shows “connected”.

### Milestone 1: Sessions + Basic Provider (API key)
- Create sessions, store messages, start runs.
- OpenAI API key path working; streaming events to GUI.

Acceptance:
- GUI chat sends message, receives streamed reply; persisted in SQLite.

### Milestone 2: Tools + Approvals (GUI + Channel)
- Implement exec/process/fs/web tools.
- Approvals work in GUI and via channel components; first decision wins.

Acceptance:
- Model requests `exec`; approval appears; approving runs command; result is persisted and shown.

### Milestone 3: Telegram Channel
- teloxide long polling + allowlist + mention gating + session mapping.
- Replies sent; chunking works.

Acceptance:
- Telegram DM and group mention produce correct session history and replies.

### Milestone 4: Discord Channel
- serenity bot + allowlist + mention gating + threads mapping.
- Approvals via buttons.

Acceptance:
- Discord thread conversation works; approvals can be done in Discord.

### Milestone 5: OAuth + Setup-Token UX
- OpenAI Codex PKCE flow with callback and paste fallback.
- Anthropic setup-token paste flow.
- Keychain storage.

Acceptance:
- GUI “Login with OpenAI Codex” completes and persists profile; provider status shows valid.

### Milestone 6: Memory Notes + Embeddings
- Notes CRUD in GUI + tool for agent to write notes (optional).
- Embeddings table populated; retrieval injects top-k.

Acceptance:
- A saved note is retrieved and cited in response when relevant.

### Milestone 7: macOS Packaging
- Bundle `.app` for GUI (macOS only target).
- Optional: GUI auto-starts gateway if not running.

Acceptance:
- User can run the app; it starts/attaches to gateway and operates normally.

---

## 14) Testing Plan

Unit tests:
- Tool argument validation + truncation/redaction
- Markdown conversion: internal -> Telegram HTML and internal -> Discord markdown chunking
- Approval resolution race (GUI+channel decision ordering)
- Vector similarity correctness and bounding

Integration tests:
- Spawn gateway in test, hit HTTP endpoints, connect WS, start run with mocked provider server
- SQLite migration tests: fresh DB and upgrade path
- Exec tool: safe commands (`echo`) and timeout/cancel behavior

Manual tests:
- Discord bot in a test guild
- Telegram bot in DM and group
- OAuth callback success and paste fallback path

---

## 15) What Transfers From OpenClaw (and What Doesn’t)
Transfers cleanly:
- OAuth PKCE + token sink concept (see `/Users/domusanimae/Documents/openclaw replacement/openclaw/docs/concepts/oauth.md`)
- Agent loop lifecycle ideas (see `/Users/domusanimae/Documents/openclaw replacement/openclaw/docs/concepts/agent-loop.md`)
- Security posture: allowlists + mention gating + approvals (see `/Users/domusanimae/Documents/openclaw replacement/openclaw/docs/gateway/security/index.md`)
- Method/event inventory as a “feature map” (see `/Users/domusanimae/Documents/openclaw replacement/openclaw/src/gateway/server-methods-list.ts`)

Does not transfer directly:
- TypeScript/TypeBox schema toolchain (we replace with Rust types + `schemars`)
- Web control UI and node/canvas ecosystem
- Any channel implementation details (Baileys/grammY/discord.js are replaced by Rust libraries)

---

## Assumptions/Defaults (Locked)
- Project name: `carsinOS` (Rust crate prefix `carsinos`)
- Packaging target: macOS only for the first shippable app; dev is `cargo run`
- Approvals: available in both GUI and chat channels
- Channels in MVP: Telegram + Discord
- Transport: HTTP commands + WS events (no JSON-RPC WS-only)
- Storage: SQLite-first with attachments on disk
- Auth: Bearer token for gateway API; provider auth is OAuth where supported + API keys fallback
