# Mission Control API Contract (Single Source of Truth)

This app is a thin React/Tauri client for the carsinOS Gateway.

If you are doing **frontend-only** work: use the existing API wrappers and do **not** invent new endpoints.

## Frontend-Only Guardrail (For Claude)
- UI/UX tasks should only touch `src/features/*`, `src/ui/*`, and `src/styles.css`.
- Treat `src/lib/api.ts`, `src/lib/ws.ts`, and `src/types.ts` as read-only unless the task explicitly requires wiring a new screen.
- Never change `crates/*` (Rust backend) in a frontend-only task.

## Where The Truth Lives
- HTTP endpoints used by the UI: `src/lib/api.ts` (do not call `fetch()` directly in components)
- WebSocket connection + event parsing: `src/lib/ws.ts` (used via `connectGatewayEvents()`)
- Frontend types used by the UI: `src/types.ts`
- Backend contract (authoritative shapes): `crates/carsinos-protocol/src/lib.rs`
- Backend routes (authoritative paths): `crates/carsinos-gateway/src/main.rs` (the `build_app(...)` route table)

## Auth + Storage Rules
- All HTTP requests require `Authorization: Bearer <token>`.
- The WebSocket uses `WS /api/v1/ws?token=<token>`.
- In Tauri, the token is stored in the OS keychain (see `src/lib/runtime.ts` and `src-tauri/src/lib.rs`).
- Never store secrets (gateway token, provider tokens) in localStorage.

## WebSocket Events
- Connect URL builder: `websocketUrlFromGateway()` in `src/lib/api.ts`
- Client connection: `connectGatewayEvents()` in `src/lib/ws.ts`
- Event frame shape: `WsEventFrame` in `src/types.ts`

Event families used for UI refresh logic:
- `job.*`, `approval.*`, `channel.*`, `extension.*` (refresh mission-control read models)
- `agent_mail.*` (refresh mail read models)
- `board.*` (board updates, asset uploads)
- `heartbeat.*` (often filtered from the operator event view)

## HTTP Endpoints Used By Mission Control

### Health + Status
- `GET /api/v1/health` (`getGatewayHealth`)
- `GET /api/v1/status` (`getGatewayStatus`)

### Boards
- `GET /api/v1/boards` (`listBoards`)
- `GET /api/v1/boards/{board_id}` (`getBoard`)
- `POST /api/v1/boards/{board_id}/cards/create` (`createBoardCard`)
- `POST /api/v1/boards/{board_id}/cards/{card_id}/update` (`updateBoardCard`)
- `POST /api/v1/boards/{board_id}/cards/{card_id}/move` (`moveBoardCard`)
- `POST /api/v1/boards/{board_id}/cards/{card_id}/run` (`runBoardCard`)
- `POST /api/v1/boards/{board_id}/cards/{card_id}/assets/upload` (`uploadBoardCardAsset`)
- `GET /api/v1/boards/{board_id}/cards/{card_id}/assets/{card_asset_id}` (`fetchBoardCardAssetBlob`)

### Agents
- `GET /api/v1/agents` (`listAgents`)

### Mission Control Read Models
- `GET /api/v1/mission-control/calendar/week` (`getMissionControlCalendarWeek`)
- `GET /api/v1/mission-control/focus?limit={n}` (`getMissionControlFocus`)

### Jobs + Scheduler
- `GET /api/v1/jobs?limit={n}&include_disabled=true` (`listJobs`)
- `GET /api/v1/jobs/status` (`getJobsStatus`)
- `POST /api/v1/jobs/{job_id}/run` (`runJobNow`)
- `POST /api/v1/jobs/{job_id}/update` (`setJobEnabledState`)

### Approvals
- `GET /api/v1/approvals?status={requested|...}&limit={n}` (`listApprovals`)
- `POST /api/v1/approvals/{approval_id}/resolve` (`resolveApproval`)

### Auth Profiles + Provider Routing
- `GET /api/v1/auth/profiles?provider={provider}&include_disabled={true|false}` (`listAuthProfiles`)
- `GET /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order` (`getAgentProviderProfileOrder`)
- `POST /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order` (`setAgentProviderProfileOrder`)

### Extensions: Skills + Plugins
- `GET /api/v1/extensions/skills?include_disabled={true|false}` (`listSkills`)
- `POST /api/v1/extensions/skills/{skill_id}/state` (`setSkillEnabled`)
- `GET /api/v1/extensions/plugins?include_disabled={true|false}` (`listPlugins`)
- `GET /api/v1/extensions/plugins/status?include_disabled={true|false}` (`listPluginRuntimeStatus`)
- `POST /api/v1/extensions/plugins/{plugin_id}/update` (`setPluginEnabled`)

### Channels Runtime
- `GET /api/v1/channels/runtime/status` (`getChannelRuntimeStatus`)
- `POST /api/v1/channels/runtime/reconnect` (`reconnectChannelRuntime`)

### Memory Notes
- `GET /api/v1/memory/notes?limit={n}` (`listMemoryNotes`)
- `POST /api/v1/memory/notes` (`createMemoryNote`)

### Agent Mail + Chatrooms
- `GET /api/v1/agent-mail/threads?...` (`listAgentMailThreads`)
- `POST /api/v1/agent-mail/threads` (`createAgentMailThread`)
- `GET /api/v1/agent-mail/threads/{thread_id}` (`getAgentMailThread`)
- `GET /api/v1/agent-mail/threads/{thread_id}/messages?limit={n}` (`listAgentMailMessages`)
- `POST /api/v1/agent-mail/threads/{thread_id}/messages` (`sendAgentMailMessage`)
- `POST /api/v1/agent-mail/messages/{message_id}/ack` (`ackAgentMailMessage`)
- `POST /api/v1/agent-mail/messages/{message_id}/attachments/upload` (`uploadAgentMailAttachment`)
- `GET /api/v1/agent-mail/messages/{message_id}/attachments/{attachment_id}` (`fetchAgentMailAttachmentBlob`)
- `GET /api/v1/agent-mail/leases?...` (`listAgentMailFileLeases`)
- `POST /api/v1/agent-mail/leases` (`createAgentMailFileLease`)
- `POST /api/v1/agent-mail/leases/{lease_id}/release` (`releaseAgentMailFileLease`)

## Adding A New Call (Frontend Only)
- First: check `src/lib/api.ts` and see if a wrapper already exists.
- If missing and the backend endpoint already exists: add a wrapper to `src/lib/api.ts` and add the corresponding type(s) in `src/types.ts`.
- If the backend endpoint does not exist: stop and file a backend ticket. Do not guess paths.
