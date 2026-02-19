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

- [ ] `O1` `MC-SEC-001` Publish and approve threat-model package (STRIDE register, asset classification, trust-boundary map, risk owners). Draft published: `docs/security/THREAT_MODEL_PACKAGE.md` (pending owner signoff).
- [ ] `O2` `MC-SEC-010` Publish human-usable incident runbooks (auth compromise, key leak, provider abuse, tool abuse, data exfil) with named ownership. Draft published: `docs/security/INCIDENT_RUNBOOKS.md` (pending owner assignment).
- [x] `O3` Security Gate 0 evidence bundling/signoff workflow: produce machine-readable release evidence that gates unresolved critical/high findings and drill outcomes. Implemented via `scripts/security_gate0_evidence_bundle.sh` + `docs/security/SECURITY_GATE0_EVIDENCE_WORKFLOW.md` + `.github/workflows/security-gate0-evidence.yml`.
- [ ] `O4` `MC-CH-010` Complete Telegram production connector path (real transport operation mode, delivery retry semantics, and production soak evidence).
- [ ] `O5` `MC-CH-020` Complete Discord production connector path (real gateway event intake, outbound operational behavior, and production soak evidence).
- [ ] `O6` Execute 7-day Telegram/Discord soak and publish resilience report (reconnect, retry, message integrity, approval round-trip).
- [ ] `O7` Complete archive-retention operational proof for security audit trail beyond 90-day hot window.
- [ ] `O8` Decide and schedule `MC-FUT-900` expansion set (if future channels continue in this wave).
- [ ] `O9` Run mandatory repository-wide hardcoded runtime-value audit and convert every deployment-specific constant to config/wizard-backed fields (`MC-CONF-005`).

## Phase P - Setup Wizard + Dynamic Configuration (MC-CONF)

- [ ] `P1` `MC-CONF-001` Freeze configuration contract for runtime-scoped values (`global`, `provider`, `auth_profile`, `channel`, `security`) with schema versioning.
- [ ] `P2` `MC-CONF-002` Implement Mission Control first-run/reconfigure wizard that captures required operator inputs without source edits.
- [ ] `P3` `MC-CONF-003` Wire wizard-driven secret references to keychain/secret backends (no plaintext secret persistence in config records).
- [ ] `P4` `MC-CONF-004` Implement config mutation audit trail + rollback to last-known-good snapshots.
- [ ] `P5` `MC-CONF-005` Enforce CI hardcoded-value guardrail with explicit allowlist + owner + expiry metadata.
- [ ] `P6` Run full regression + benchmark + security gate suite after MC-CONF implementation.

## Owner Inputs Required (Prevents Hard Blockers)

- [ ] `R1` JWT/API gateway production contract values: issuer allowlist, audience values, trusted proxy/header policy, TLS termination model.
- [ ] `R2` Telegram production integration details: bot token strategy, webhook domain vs long-poll decision, allowlist seed policy, staging chat IDs.
- [ ] `R3` Discord production integration details: bot token/app IDs, required intents, staging guild/channel IDs, role/permission model.
- [ ] `R4` Security ownership and signoff authority: threat-model approver, risk acceptance owner, release-block exception owner.
- [ ] `R5` Incident operations ownership: on-call escalation map and authority boundaries for profile/provider/global kill-switch actions.
- [ ] `R6` Audit retention/archive target: backend destination, encryption/KMS requirement, retrieval SLA, immutable retention policy.
- [ ] `R7` Consumer OAuth production stance: enabled/disabled modes, approved operator warning text, explicit risk acceptance decision.
- [ ] `R8` Priority order for `MC-FUT-900` channels (only needed if expansion continues now).

## Verification Snapshot (2026-02-19)

- [x] `V1` Confirmed `MC-SEC-002` through `MC-SEC-010` runtime controls and test gates are implemented in repository.
- [x] `V2` Confirmed security automation scripts/workflows exist: `scripts/security_pr_gate.sh`, `scripts/security_nightly_deep_scan.sh`, `scripts/security_killswitch_drill.sh`, `scripts/security_secret_lifecycle_drill.sh`, plus GitHub workflows.
- [x] `V3` Confirmed channel adapter scaffolds for `MC-FUT-010..050` exist (WhatsApp, Slack, iMessage/BlueBubbles, Signal, Twitch).
- [x] `V4` Confirmed no open PR backlog remains after convergence/cleanup merge wave.
- [x] `V5` Confirmed threat-model artifact set and formal incident runbook docs are not yet published in-repo and remain blocker items.
