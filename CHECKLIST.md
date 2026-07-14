# CHECKLIST.md

Execution checklist derived from `PLAN.md`. IDs are stable and must be used by `CHECKPOINT.md`.

## Phase A - Foundation

- [x] `A1` Initialize Rust workspace and core crates.
- [x] `A2` Add gateway bootstrap with token auth and health/status endpoints.
- [x] `A3` Add SQLite migration bootstrap and initial schema.
- [x] `A4` Run baseline compile + smoke tests for Milestone 0.

## Phase B - Gateway Session Core

- [x] `B1` Add storage repositories for sessions, messages, runs.
- [x] `B2` Add protocol request/response types for session/run APIs.
- [x] `B3` Implement HTTP endpoints:
- `GET /api/v1/sessions`
- `POST /api/v1/sessions`
- `GET /api/v1/sessions/{session_id}`
- `POST /api/v1/sessions/{session_id}/messages`
- `POST /api/v1/sessions/{session_id}/runs`
- [x] `B4` Implement run state bootstrap (`queued` initial state creation).
- [x] `B5` Add gateway integration tests for auth + session CRUD + run creation.
- [x] `B6` Full regression suite for Phase B.

## Phase C - Provider Abstraction + First Model Path

- [x] `C1` Create `carsinos-providers` crate with provider trait and OpenAI adapter skeleton.
- [x] `C2` Wire run execution path to provider interface (streaming text deltas via WS events).
- [x] `C3` Persist assistant outputs into `messages`.
- [x] `C4` Add provider mock tests and gateway streaming tests.
- [x] `C5` Full regression suite for Phase C.

## Phase D - Tools and Approvals

- [x] `D1` Create `carsinos-tools` crate (`exec`, `process`, `fs.read`, `fs.write`, `web.search`, `web.fetch`).
- [x] `D2` Add policy + approval gate state machine.
- [x] `D3` Add approval endpoints and WS events.
- [x] `D4` Add strict tests for truncation, timeouts, cancellations, race conditions.
- [x] `D5` Full regression suite for Phase D.

## Phase E - Channels

- [x] `E1` Add Telegram crate adapter scaffolding and channel mapping rules.
- [x] `E2` Add Discord crate adapter scaffolding and thread/channel mapping rules.
- [x] `E3` Add mention gating + allowlist authz contracts for channel actions.
- [x] `E4` Add channel approval interaction payload contracts (buttons/inline callback IDs).
- [x] `E5` Full regression suite for Phase E.

## Phase F - GUI

- [x] `F1` Create `carsinos-gui` crate (`egui/eframe`) and gateway client.
- [x] `F2` Build screens: status, sessions, chat stream, approvals.
- [x] `F3` Add markdown rendering baseline.
- [x] `F4` Add provider auth screens and channel config screens.
- [x] `F5` Full regression suite for Phase F.

## Phase G - OAuth/Auth UX

- [x] `G1` Implement OpenAI Codex OAuth PKCE start/finish flow.
- [x] `G2` Implement Anthropic setup-token ingest flow.
- [x] `G3` Store secrets in keychain + metadata in sqlite/config.
- [x] `G4` Add auth flow tests (including callback fallback and error paths).
- [x] `G5` Full regression suite for Phase G.

## Phase H - Memory + Embeddings

- [x] `H1` Add notes CRUD and embedding storage/retrieval pipeline.
- [x] `H2` Add retrieval policy and bounded prompt injection.
- [x] `H3` Add ranking/search tests and persistence tests.
- [x] `H4` Full regression suite for Phase H.

## Phase I - Packaging + Operational Hardening

- [x] `I1` Add macOS app packaging path and launch behavior.
- [x] `I2` Add production logging/metrics/health hardening.
- [x] `I3` Add upgrade migration validation tests.
- [x] `I4` Final full regression suite across workspace.

## Cross-Cut Quality Gates

- [x] `Q1` Add process-level HTTP E2E tests (real gateway process, restart persistence).
- [x] `Q2` Add process-level WS E2E tests for run/approval event stream.
- [x] `Q3` Add benchmark suite with latency percentile thresholds.
- [x] `Q4` Run full regression + benchmark commands and record results.
- [x] `Q5` Expand process-level test matrix (negative paths, concurrency, log persistence checks).
- [x] `Q6` Implement robust, filterable logging system (request IDs, structured formatting, file sinks).
- [x] `Q7` Validate expanded tests + logging via full regression and benchmark capture.

## Phase J - AppDex Foundation (Sprint 1)

- [x] `J1` Implement provider/auth risk-control foundation (`A0`) including auth-mode registry and kill-switch controls.
- [x] `J2` Implement real provider manager baseline (`A1`) with OpenAI/Anthropic HTTP adapters and normalized provider error codes.
- [x] `J3` Implement auth profile system baseline (`A2`) with profile CRUD, metadata fields, and per-agent profile ordering.
- [x] `J4` Implement token refresh + fallback policy scaffolding (`A3`) with auth-mode-constrained fallback behavior.
- [x] `J5` Full regression + benchmark suite for Sprint 1 changes.

## Phase K - Scheduler & Automation (Sprint 2)

- [x] `K1` Add durable job schema and storage APIs (jobs + run history + lease/update primitives).
- [x] `K2` Add gateway job APIs (`list/status/add/update/remove/run/history`) with auth + validation.
- [x] `K3` Add scheduler engine loop with due-job pickup, lease/timeout recovery, and deterministic event emission.
- [x] `K4` Add scheduler execution integration with bounded retries and persisted run outcomes.
- [x] `K5` Full regression + benchmark suite for Sprint 2 scheduler changes.

## Phase L - Tool Runtime Completion (Sprint 3)

- [x] `L1` Implement `process`, `web_search`, and `web_fetch` tool handlers with bounded timeout/truncation semantics.
- [x] `L2` Add run-engine tool-call loop integration for multi-step tool execution with persisted tool outputs.
- [x] `L3` Extend approval gating for high-risk tool calls across the tool loop execution path.
- [x] `L4` Add strict tests for tool loop branching, error handling, timeout/cancel paths, and approval resume/deny.
- [x] `L5` Full regression + benchmark suite for Sprint 3 tool-runtime changes.

## Phase M - Numquam Integration Adapter (Sprint 5)

- [x] `M1` Add Numquam integration client with canonical `integration.v1` envelope handling and transport selection (`http` or `mcp`).
- [x] `M2` Fetch memory context before provider completion (`context/build`) with correlation-id propagation.
- [x] `M3` Submit post-run memory proposal (`writeback/propose`) with evidence metadata capture.
- [x] `M4` Implement degrade-safe fallback (`degrade_mode` + `fallback_recommendation=stateless_chat`) with operator-visible warnings.
- [x] `M5` Add strict integration tests (contract success/failure, degrade/fallback, writeback invocation) and run full regression + benchmark suite.

## Phase N - Security Hardening Runtime (MC-SEC)

- [x] `N1` `MC-SEC-002` Implement JWT edge identity validation with stable auth error taxonomy and role claims normalization.
- [x] `N2` `MC-SEC-003` Enforce role matrix on high-risk endpoint surfaces (auth profile mutation, run/approval/job/channel mutations, security audit read).
- [x] `N3` `MC-SEC-005` Enforce public bind/TLS/trusted-proxy policy gates and fail-closed spoof protection.
- [x] `N4` `MC-SEC-006` Enforce deterministic per-IP/per-principal/per-endpoint request rate limits with stable `RATE_LIMITED` responses.
- [x] `N5` `MC-SEC-007` Enforce tool runtime containment (allowed filesystem roots, binary allowlist, network allowlist/deny policy) with deny-path tests.
- [x] `N6` `MC-SEC-008` Implement persistent security audit ledger + query API (`GET /api/v1/security/audit`) with mutation/authz deny audit coverage.
- [x] `N7` `MC-SEC-004` Expand secret/key lifecycle automation with explicit rotation/revocation scheduling and non-interactive drill harnesses.
- [x] `N8` `MC-SEC-009` Add per-PR + nightly supply-chain/vulnerability scan automation scripts and enforcement hooks.
- [x] `N9` `MC-SEC-010` Implement incident-response drill execution harness and measurable kill-switch operation runbooks.
- [x] `N10` Run post-hardening regression suite (`clippy`, `test`, `benchmark`) and capture outputs for checkpoint signoff.

## Phase O - Remaining Work (Verified Blockers Only)

- [x] `O1` `MC-SEC-001` Publish and approve threat-model package (STRIDE register, asset classification, trust-boundary map, risk owners). Owner signoff captured in `docs/security/THREAT_MODEL_PACKAGE.md` (approver/owner: `ProfessahX`).
- [x] `O2` `MC-SEC-010` Publish human-usable incident runbooks (auth compromise, key leak, provider abuse, tool abuse, data exfil) with named ownership. Ownership captured in `docs/security/INCIDENT_RUNBOOKS.md` (primary/backup: `ProfessahX`).
- [x] `O3` Security Gate 0 evidence bundling/signoff workflow: produce machine-readable release evidence that gates unresolved critical/high findings and drill outcomes. Implemented via `scripts/security_gate0_evidence_bundle.sh` + `docs/security/SECURITY_GATE0_EVIDENCE_WORKFLOW.md` + `.github/workflows/security-gate0-evidence.yml`.
- [x] `O11` `MC-CH-001` Add shared channel adapter lifecycle contract in `carsinos-core` to formalize `start/stop/reconnect/health` behavior for Telegram/Discord runtime adapters.
- [x] `O12` `MC-CH-002` Add gateway channel runtime manager foundation with supervisor loop + status/reconnect endpoints (`GET /api/v1/channels/runtime/status`, `POST /api/v1/channels/runtime/reconnect`) and role/audit test coverage.
- [x] `O4` `MC-CH-010` Complete Telegram production connector path (real transport operation mode + delivery retry semantics + inbound/outbound run roundtrip behavior). Soak signoff evidence is tracked under `O6`.
- [x] `O5` `MC-CH-020` Complete Discord production connector path (real gateway event intake + outbound operational behavior + inbound/outbound run roundtrip behavior). Soak signoff evidence is tracked under `O6`.
- [ ] `O6` Execute 7-day Telegram/Discord soak and publish resilience report (reconnect, retry, message integrity, approval round-trip). Harness/runbook/workflow published: `scripts/channel_soak_runner.py`, `docs/channels/CHANNEL_SOAK_RUNBOOK.md`, `.github/workflows/channel-soak.yml`; local smoke validation is green in shim mode (`runtime/channels/reports/channel-soak-20260220T061839Z.json`), pending full 7-day live signoff window.
- [x] `O7` Complete archive-retention operational proof for security audit trail beyond 90-day hot window. Implemented via `scripts/security_archive_retention_proof.sh`, `.github/workflows/security-archive-retention-proof.yml`, and `docs/security/ARCHIVE_RETENTION_OPERATIONAL_PROOF.md`.
- [x] `O8` Decide and schedule `MC-FUT-900` expansion set (if future channels continue in this wave). Owner decision: deferred out of current wave; revisit in next planning cycle.
- [x] `O9` Run mandatory repository-wide hardcoded runtime-value audit and convert every deployment-specific constant to config/wizard-backed fields (`MC-CONF-005`). Audit report published: `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`.
- [x] `O10` Consume and triage hardcoded-value audit findings into implementation tickets by config scope (`global`, `provider`, `auth_profile`, `channel`, `security`) with owner + target milestone. Ticketization added in `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md` + `APPDEX_IMPLEMENTATION_TICKET_PACK.md` (`MC-CONF-006..009`).

## Phase P - Setup Wizard + Dynamic Configuration (MC-CONF)

- [x] `P1` `MC-CONF-001` Freeze configuration contract for runtime-scoped values (`global`, `provider`, `auth_profile`, `channel`, `security`) with schema versioning. Implemented via `runtime.config.v1` typed contract + `GET/POST /api/v1/config/runtime`.
- [x] `P2` `MC-CONF-002` Implement Mission Control first-run/reconfigure wizard that captures required operator inputs without source edits. Implemented in `carsinos-gui` Mission tab with runtime-config fetch/parse/save/rollback, step-level validation, completeness gate, and high-risk OAuth lock enforcement until required fields are complete.
- [x] `P3` `MC-CONF-003` Wire wizard-driven secret references to keychain/secret backends (no plaintext secret persistence in config records). Implemented via runtime secret upsert/delete API (`POST /api/v1/config/runtime/secrets/upsert`, `POST /api/v1/config/runtime/secrets/delete`) plus rotation/delete integration test coverage.
- [x] `P4` `MC-CONF-004` Implement config mutation audit trail + rollback to last-known-good snapshots. Implemented via runtime config hash-audited mutations + `POST /api/v1/config/runtime/rollback`.
- [x] `P5` `MC-CONF-005` Enforce CI hardcoded-value guardrail with explicit allowlist + owner + expiry metadata. Implemented via `scripts/security_hardcoded_value_guard.py` + `docs/security/HARDCODED_VALUE_ALLOWLIST.csv` + PR gate integration.
- [x] `P6` Run full regression + benchmark + security gate suite after MC-CONF implementation. Completed via `cargo test --workspace --locked` and `scripts/security_pr_gate.sh` (with `cargo-audit` installed/enabled).

## Phase Q - Autonomy Guardrails (AG)

- [x] `AG-001` Guardrail config contract: add `RuntimeAutonomyGuardrailsConfig` defaults/validation and runtime config API round-trip coverage.
- [x] `AG-002` Per-session run lane lock: enforce one active run per session for create/resume/channel/scheduler entrypoints; interactive conflicts return `409`.
- [x] `AG-003` Run budget governor: enforce max wall time/tool calls/provider input chars/tool output chars/provider attempts with stable terminal reason codes.
- [x] `AG-004` Scheduler timeout enforcement: enforce `job.timeout_ms` via `tokio::time::timeout` around `execute_job_payload`.
- [x] `AG-005` Single scheduler instance guard: add lock ownership guard so only one scheduler loop runs per state dir.
- [x] `AG-011-P0` Regression subset: add P0 E2E runaway prevention coverage (lane lock conflict, scheduler timeout, duplicate scheduler guard, core budget stop).
- [x] `AG-006` Tool fanout cap + repeated tool error loop breaker.
- [x] `AG-007` Token/cost accounting + daily per-profile budget kill switch.
- [x] `AG-008` Heartbeat mode (`heartbeat.run`) with runtime-enforced no-tools policy and output contract.
- [x] `AG-009` Provider/job circuit breakers with open/skip/reset behavior.
- [x] `AG-010` Observability and operator UX: expose guardrail/breaker/scheduler lock state and stop reasons in status/job status/events.
- [x] `AG-011` Full runaway-prevention E2E regression suite + benchmark confirmation.

## Phase R - Core Loop Adds (Scratchpad)

- [x] `RCORE-001` Always-on listener ingestion loops for Telegram/Discord in gateway runtime with internal auth isolation and cursor tracking.
- [x] `RCORE-002` Scheduler depth upgrade with richer schedule modes (`at`, `every`, `cron`) and clear routine controls.
- [x] `RCORE-003` Production trust contract finalization for internet-facing edge (`issuer`, `audience`, proxy/TLS model) and deployment lock file.
- [x] `RCORE-004` Channel action depth completion (`pin`, `reaction`) with transport-aware capability fallback and explicit operator visibility.
- [x] `RCORE-005` Extension runtime phase-2 hardening (`install/update/rollback` lifecycle + safety controls) with strict regression gates.

## Owner Inputs Required (Prevents Hard Blockers)

- [x] `R1` JWT/API gateway production contract values: issuer allowlist, audience values, trusted proxy/header policy, TLS termination model. Resolved via runtime trust-lock contract (`state_dir/deployment/trust_contract.lock.json`) with wizard/API-driven updates and lock-sync enforcement.
- [x] `R2` Telegram production integration details: bot token strategy, webhook domain vs long-poll decision, allowlist seed policy, staging chat IDs. Owner decision: `long_poll` local-first; allowlist seed set; direct-chat local soak may use operator user ID as staging chat ID.
- [x] `R3` Discord production integration details: bot token/app IDs, required intents, staging guild/channel IDs, role/permission model. Owner decision: use wizard defaults for intents and local-first staging; channel + allowlisted author IDs provided.
- [x] `R4` Security ownership and signoff authority: threat-model approver, risk acceptance owner, release-block exception owner. Assigned owner alias: `ProfessahX`.
- [x] `R5` Incident operations ownership: on-call escalation map and authority boundaries for profile/provider/global kill-switch actions. Owner decision: `ProfessahX` (single-owner model for current phase).
- [x] `R6` Audit retention/archive target: backend destination, encryption/KMS requirement, retrieval SLA, immutable retention policy. Owner decision: local archive target accepted (`90-day hot + local archive` policy retained).
- [x] `R7` Consumer OAuth production stance: enabled/disabled modes, approved operator warning text, explicit risk acceptance decision. Owner decision: `enabled` (high-risk mode with warning/audit/kill-switch controls).
- [x] `R8` Priority order for `MC-FUT-900` channels (only needed if expansion continues now). Owner decision: defer channel expansion prioritization until next planning cycle.

## Verification Snapshot (2026-02-19)

- [x] `V1` Confirmed `MC-SEC-002` through `MC-SEC-010` runtime controls and test gates are implemented in repository.
- [x] `V2` Confirmed security automation scripts/workflows exist: `scripts/security_pr_gate.sh`, `scripts/security_nightly_deep_scan.sh`, `scripts/security_killswitch_drill.sh`, `scripts/security_secret_lifecycle_drill.sh`, plus GitHub workflows.
- [x] `V3` Confirmed channel adapter scaffolds for `MC-FUT-010..050` exist (WhatsApp, Slack, iMessage/BlueBubbles, Signal, Twitch).
- [x] `V4` Confirmed no open PR backlog remains after convergence/cleanup merge wave.
- [x] `V5` Confirmed threat-model artifact set and formal incident runbook docs are published, owner-assigned, and reflected in checklist closure state (`O1`, `O2`).

## Phase S - Next Buildout Execution (MC-EXT-NEXT)

- [x] `S1` `MC-EXT-NEXT-001` Implement plugin manifest v2 schema + validation (local artifact path, sha256 verification, daemon allowlist enforcement, tool-name collision rejection).
- [x] `S2` `MC-EXT-NEXT-002` Implement plugin storage layout (`bundles/`, `active/`, `pointers/`) with atomic install/update and single-depth rollback pointer semantics.
- [x] `S3` `MC-EXT-NEXT-003` Implement plugin runner contract v1 request/response envelopes and strict stdout parsing contract.
- [x] `S4` `MC-EXT-NEXT-004` Implement subprocess invoke path with timeout/output limits and sanitized execution envelope.
- [x] `S5` `MC-EXT-NEXT-005` Wire hook execution (`run.start`, `run.end`, `tool.before`, `tool.after`) through runner contract with failure isolation.
- [x] `S6` `MC-EXT-NEXT-006` Implement per-plugin health/breaker state (3 failures, 15-minute cooldown) and status surfaces.
- [x] `S7` `MC-EXT-NEXT-007` Implement daemon runner mode with runtime allowlist and restart/timeout supervision semantics.
- [x] `S8` `MC-EXT-NEXT-008` Implement plugin-defined tool registration/execution with high-risk + approval-required defaults.
- [x] `S9` `MC-TOOLS-NEXT-001` Add global tool capability endpoint for core + plugin tools.
- [x] `S10` `MC-TOOLS-NEXT-002` Add tool conformance harness for timeout/truncation/sandbox/approval deny paths.
- [x] `S11` `MC-PROV-NEXT-001..003` Freeze provider contract, add provider conformance harness, and enforce run-loop modular boundaries.
- [x] `S12` `MC-CH-FUT-001` Publish future channel adapter template + shim E2E harness (no concrete target channel).
- [x] `S13` Execute full regression + benchmark gates after each PR chunk and at Phase S final ship gate.

## Phase T - Numquam Integration Hardening (MC-MNO)

- [x] `T1` `MC-MNO-001` Add runtime `memory.numquam` config contract + secret-ref wiring with runtime-first resolution and env compatibility fallback.
- [x] `T2` `MC-MNO-002` Add startup/runtime handshake checks (`health.get`, `capabilities.get`) with contract validation + degrade-state surfaces in `/api/v1/status` and `/api/v1/jobs/status`.
- [x] `T3` `MC-MNO-003` Add deterministic context policy engine for `context.build` (`top_k`, `risk_signal`, `message_window`, preference/query hints) and per-run policy metadata logging.
- [x] `T4` `MC-MNO-004` Add provider-input safety cap for Numquam context with deterministic truncation metadata and warning flags.
- [x] `T5` `MC-MNO-005` Add Numquam failure breaker (`numquam:integration`) with cooldown/reset behavior and run-level fallback guardrail events.
- [x] `T6` `MC-MNO-006` Add writeback payload quality enforcement + stable idempotency behavior in propose path.
- [x] `T7` `MC-MNO-007` Add MNO observability/audit coverage for handshake, fallback, breaker transitions, and explainability operations.
- [x] `T8` `MC-MNO-008` Extend Numquam regression suite (breaker open, context truncation, explainability endpoint) and keep process-level integration tests green.
- [x] `T9` `MC-MNO-009` Add operator explainability API surface (`POST /api/v1/runs/{run_id}/memory/why`) backed by `context.why`.
- [x] `T10` `MC-MNO-010` Add `memory.md` sync pipeline (`POST /api/v1/memory/sync` + scheduler mode `memory.sync`) with source-path + last-sync metadata tracking.
- [x] `T11` `MC-MNO-011` Add runtime-configurable blend policy (`mno_primary`, `local_fallback_only`, `local_augment`) with effective mode persisted in run usage metadata.
- [x] `T12` `MC-MNO-012` Add scheduled MNO ops modes (`memory.preflight`, `memory.parity_probe`) with fail-safe behavior and event/audit evidence.
- [x] `T13` `MC-MNO-013` Add explicit MNO pipeline hook mode (`memory.pipeline.hook`) and operator runbook docs.
- [x] `T14` `MC-MNO-014` Publish soak-readiness checklist and rollback plan docs for multi-day MNO validation.
- [x] `T15` Full regression + benchmark gates for MC-MNO track.

## Phase U - Mission Control v3 Pipeline-First (MC3 P0)

- [x] `U1` `MC3-BE-001` Add Agents API surfaces + bootstrap seed for `lyra` and `claude`.
- [x] `U2` `MC3-BE-002` Add boards/columns/cards schema + CRUD/move/order APIs with built-in Tasks and Content boards.
- [x] `U3` `MC3-BE-003` Add card asset upload/attachment contract with safe path handling and size/type caps.
- [x] `U4` `MC3-BE-004` Emit realtime board/card change events over websocket.
- [x] `U5` `MC3-BE-005` Add card run hook (`card -> session -> run`) with persisted linkage metadata.
- [x] `U6` `MC3-UI-001` Implement new Mission Control v3 shell/navigation (pipeline-first layout, dual-agent indicators).
- [x] `U7` `MC3-UI-002` Implement Tasks board UI (create/edit/move/assign/realtime refresh).
- [x] `U8` `MC3-UI-003` Implement Content board UI (pipeline stages + script/asset handling).
- [x] `U9` `MC3-UI-004..007` Implement Calendar, Memory, Team, and Approvals pages for v3 shell.
- [x] `U10` Run full regression + benchmark gates for MC3 P0 and capture checkpoint evidence.

## Phase V - Mission Control v3 Safe Automation (MC3 P1)

- [x] `V1` `MC3-AUTO-001` Add per-column automation rules (disabled by default) with schedule, assigned agent, run caps, retry caps, and breaker window.
- [x] `V2` `MC3-AUTO-001` Add operator controls (`run now`, pause/resume, quick disable) through API + GUI surfaces.
- [x] `V3` `MC3-AUTO-002` Implement scripting->thumbnail automation job flow with card updates and attachment writeback.
- [x] `V4` `MC3-AUTO-002` Enforce no-runaway guardrails (max attempts/day, breaker, audit trail, explicit stop reasons).
- [x] `V5` Extend regression/e2e/benchmark coverage for automation flows and verify existing suites remain green.

## Phase W - MC Slick + Agent Mail (PR-B)

- [x] `W1` `MC-SLICK-APP-001` Create `apps/mission-control` Tauri + React + Vite skeleton with one-command dev scripts.
- [x] `W2` `MC-SLICK-APP-002` Implement config-first connection/auth UX (gateway URL, token secret storage, health/reconnect actions).
- [x] `W3` `MC-SLICK-APP-003` Implement websocket realtime client with incremental event routing by `event_type`.
- [x] `W4` `MC-SLICK-APP-010` Implement Kanban board UI with virtualization + drag/drop + optimistic update rollback.
- [x] `W5` `MC-SLICK-APP-011` + `MC-SLICK-APP-012` Implement card drawer run hook and asset upload/preview using secure asset fetch endpoint.
- [x] `W6` Run phase regression gates and benchmark suite, then complete PR workflow (open, CR loop, merge, post-merge checkpoint).

## Phase X - MC Slick P1 (PR-C)

- [x] `X1` `MC-SLICK-BE-101` Add stable mission-control read model APIs for calendar week view and operator focus queue.
- [x] `X2` `MC-SLICK-APP-101` Implement Mission Control Calendar tab with week board, always-running lane, next-up queue, and run/pause/resume actions.
- [x] `X3` `MC-SLICK-APP-102` Implement Operator Focus tab with actionable queue (approvals, failures, channel health, breaker alerts).
- [x] `X4` `MC-SLICK-APP-103` Implement Event Stream tab backed by websocket feed with default noise filtering and raw toggle.
- [x] `X5` Run PR-C regression + benchmark + security gates and complete PR workflow (open, CR loop, merge, post-merge checkpoint).

## Phase Y - MC Slick P2 Cockpit (PR-D)

- [x] `Y1` `MC-SLICK-APP-201` Implement cockpit widget registry/palette/grid with save/load pages and import/export JSON.
- [x] `Y2` `MC-SLICK-APP-202` Implement pinned health strip with incident-mode filtering and top-priority break/fail indicators.
- [x] `Y3` `MC-SLICK-APP-203` Implement multi-agent/provider control surfaces (profile order, channel ops, skill/plugin toggles).
- [x] `Y4` Run PR-D regression + benchmark + security gates and complete PR workflow (open, CR loop, merge, post-merge checkpoint).

## Phase Z - Agent Mail P3 (PR-E)

- [x] `Z1` `AM-BE-001` Add agent-mail persistence/search schema + storage APIs with attachment policy controls.
- [x] `Z2` `AM-BE-002..004` Add agent-mail HTTP/WS APIs (threads/messages/ack/chatrooms) with guardrails/rate limits/audit trails.
- [x] `Z3` `AM-APP-001` Implement Mission Control Mail page (inbox/outbox/thread/compose/search/summarize).
- [x] `Z4` `AM-APP-002` Implement Mission Control Chatrooms page with realtime stream + moderation controls.
- [x] `Z5` `AM-OPT-001` Implement optional advisory file leases (TTL/exclusive/conflict visibility).
- [x] `Z6` Run PR-E regression + benchmark + security gates and complete PR workflow (open, CR loop, merge, post-merge checkpoint).

## Phase AA - Agent Mail MCP Facade (PR-F Optional)

- [x] `AA1` `AM-MCP-001` Implement MCP-compatible facade endpoints for agent-mail send/fetch/ack/leases.
- [x] `AA2` Run PR-F regression + benchmark + security gates and complete PR workflow (open, CR loop, merge, post-merge checkpoint).

## Phase AB - Mission Control App.tsx Refactor + Modularization

- [x] `AB0` Validate baseline gates (`npm run typecheck`, `npm run lint`, `npm run build`, `cargo check`) before structural extraction.
- [x] `AB1` Extract pure helper modules (`utils/*`, `features/boards/boardModel.ts`, `features/cockpit/cockpitLayout.ts`, `features/agentMail/agentMailSummary.ts`) and keep behavior stable.
- [x] `AB2` Extract `BoardLane` into `features/boards/BoardLane.tsx` with localized lint suppression for `useVirtualizer`.
- [x] `AB3` Split tab JSX into page components (`boards`, `calendar`, `focus`, `events`, `agentMail`, `chatrooms`, `cockpit`) without moving controller logic yet.
- [x] `AB4` Add feature controllers + app controller (`app/useAppController.ts`, `features/*/use*Controller.ts`) and thin `App.tsx`.
- [x] `AB4a` Extract app-shell state (`activeTab`, connection drafts, token/ws/notice, event stream) into `app/useAppController.ts`.
- [x] `AB5` Move websocket wiring to `app/useGatewayEvents.ts` with stable callback handling and cleanup guarantees.
- [x] `AB6` Add shared UI primitives in `src/ui/*`, run full regression gates, and execute PR chunk workflow.

## Phase AC - Windows Public Beta Release (v0.1.0-beta)

- [x] `AC1` Reconcile the nightly root audit exception and add an unignored audit for the nested Tauri lockfile.
- [x] `AC2` Bundle the authenticated loopback gateway as a managed Mission Control sidecar with stable per-user state.
- [x] `AC3` Add checksum-verified Windows MSI packaging and prove install, launch, clean shutdown, uninstall, and state preservation.
- [x] `AC4` Add a manifest-verified backup, verify, and rollback-safe restore workflow that excludes credentials and ephemeral files.
- [x] `AC5` Reconcile public README, security, install, release-note, checklist, and blockerboard truth.
- [x] `AC6` Run full local frontend, Rust, security, packaging, backup, and visual QA gates.
- [x] `AC7` Open the release PR, pass independent GitHub/CodeRabbit review, merge to `main`, and enable repository security controls.
- [x] `AC8` Tag and publish `v0.1.0-beta` with checksum-verified Windows artifacts, verify release assets, and synchronize local `main`.

The provider/channel/scheduler soak is explicitly excluded from this beta goal by owner direction.
