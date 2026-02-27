# AppDex Executable Ticket Backlog: NumquamOblita MNO Integration in carsinOS

Date: 2026-02-27
Owner: AppDex
Scope: Make NumquamOblita (MNO) the primary memory system for carsinOS runs.

## Objective
Ship production-grade MNO integration with safe fallback behavior, operator visibility, and guardrails that prevent prompt/context blowups and repeated-call churn. Local notes remain a fast fallback/augment, not the primary source of truth.

## Source of Truth (Contract + Reference Impl)
- Integration Contract v1: NumquamOblita `docs/CARSINOS_INTEGRATION_REQUIREMENTS.md`
- HTTP/MCP reference server: NumquamOblita `engine/runtime/server.py` and `engine/mcp/server.py`

Canonical operation ids (v1): `context.build`, `context.why`, `writeback.propose`, `writeback.resolve`, `health.get`, `capabilities.get`.

## Core Run Order (Do Not Reorder)
tools -> `context.build` -> provider -> `writeback.propose` -> operator approval -> `writeback.resolve`

## Definition of Done
1. Every run path (manual session, channel ingest, scheduler `session.run`) invokes `context.build` before provider when MNO is enabled.
2. Any MNO failure or `degrade_mode=true` yields stateless/local fallback without blocking a run.
3. `writeback.propose` uses required `Idempotency-Key`; retries are deterministic; `writeback.resolve` is idempotent (`already_resolved`).
4. carsinOS can call all six ops and records correlation ids (`request_id`, `request_id_source`) on runs.
5. MNO integration is runtime-configurable (via `/api/v1/config/runtime` + runtime secrets), not hardcoded env-only.

## Locked Constraints
1. Keep MNO as a sidecar service via Integration v1 (HTTP/MCP). No Rust port of MNO.
2. Preserve existing run-order: tools -> memory context -> provider -> writeback proposal.
3. Never disable fallback path.
4. Do not store raw MNO tokens/secrets in runtime config; use runtime secret refs.

## Phase Order (Strict)
1. P0 Foundation and safety hardening.
2. P1 Operator surfaces and memory.md sync.
3. P2 Orchestration and soak readiness.

## P0 Tickets (Must Ship First)

### MC-MNO-001 Runtime Config Surface + Secret Wiring
Priority: P0
Files: `crates/carsinos-protocol/src/lib.rs`, `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Add runtime config schema for MNO (suggested shape under `global.memory.numquam` or similar):
   - `enabled` (bool)
   - `integration_base_url` (string)
   - `transport` (string enum: `http|mcp|dual`; note: `dual` is carsinOS-only)
   - timeouts (ms): `context_build_timeout_ms`, `writeback_propose_timeout_ms`, `writeback_resolve_timeout_ms`, `handshake_timeout_ms`
   - `token_secret_ref` (string; fetched via `/api/v1/config/runtime/secrets/*`)
   - optional metadata: `principal_id`, `principal_display_name` (metadata only; auth is token-derived)
2. Load from runtime config first; keep env as compatibility fallback.
3. Token updates via secret ref must take effect without gateway restart.
Acceptance:
1. `GET /api/v1/config/runtime` and `POST /api/v1/config/runtime` round-trip the new MNO config.
2. When `token_secret_ref` changes, subsequent MNO calls use the new token without restart.

### MC-MNO-002 Startup Contract Handshake + Capability/Health Gating
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Implement client calls for `GET /api/integration/v1/health` and `GET /api/integration/v1/capabilities` (HTTP) and MCP equivalents.
2. On startup and on a periodic interval, call capabilities + health and validate:
   - `schema_version == integration.v1`
   - `supported_schema_versions` contains `integration.v1`
   - required operations exist and are `enabled=true`
   - selected transport is supported (`http` or `mcp`; `dual` uses both)
3. Expose a clear degrade/disabled state when contract mismatch, auth failure, or health down.
Acceptance:
1. Contract mismatch (missing op / unsupported schema) forces safe fallback mode.
2. `GET /api/v1/status` (or existing status surfaces) includes MNO: enabled, transport, health status, contract_version, degrade state.

### MC-MNO-003 Context Request Policy Engine
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`
Deliver:
1. Replace fixed defaults with policy-driven request shaping for `context.build`:
   - `data.retrieval.top_k` (1..30)
   - `data.risk_signal` (`low|medium|high`)
   - optional `data.message_window` (`max_messages`, `max_chars`)
   - optional `data.memory_preference`, `data.retrieval_query`
   - always provide `data.message` (so servers that cannot derive a message from history still work)
2. Policy inputs: channel type, run mode (manual/scheduled), autonomy guardrails.
Acceptance:
1. Policy decisions are deterministic and logged per run.
2. Tests cover low/medium/high routing and top_k selection.

### MC-MNO-004 Context Budget Guard (Provider Input Safety)
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Hard-cap MNO `context_text` contribution before provider input assembly.
2. Tie the cap to existing autonomy guardrails (for example: never exceed `autonomy_guardrails.max_provider_input_chars`, and reserve headroom for tool output + user text).
3. Persist truncation metadata on the run (original vs returned sizes, warning flags).
Acceptance:
1. Oversized context is truncated deterministically with metadata and a warning flag.
2. Runs do not fail due to avoidable provider-input overflow.

### MC-MNO-005 MNO Failure Breaker (Avoid Repeated Timeouts)
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`
Deliver:
1. Add a simple breaker: after N consecutive MNO failures/timeouts, skip MNO calls for a cool-down window (continue stateless/local fallback).
2. Breaker must reset on a successful handshake or successful `context.build`.
3. Expose breaker state in status/metrics.
Acceptance:
1. When MNO is down, carsinOS stops waiting on per-run timeouts after breaker trips.
2. When MNO recovers, breaker clears and MNO resumes without restart.

### MC-MNO-006 Writeback Hardening
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Use stable idempotency keys for `writeback.propose` (header `Idempotency-Key`) based on run identity.
2. Enforce proposal payload quality before sending:
   - `data.mutation` present with `intent`, `target_kind`, and `body`/`target_id` as required
   - `data.evidence` non-empty and includes required fields (`provenance_handle`, `source_kind`, `source_id`, `excerpt`, `citation`, `confidence`)
3. Ensure repeated approval resolves are safe and surface `already_resolved`.
Acceptance:
1. Replayed writeback proposals are stable and non-duplicative.
2. Repeated resolve calls are no-op safe (`already_resolved=true`) and auditable.

### MC-MNO-007 Observability + Audit Contract (carsinOS Side)
Priority: P0
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`
Deliver:
1. Persist on each run: `request_id`, `request_id_source`, `degrade_mode`, warning codes, route/confidence, evidence ids.
2. Emit explicit audit events for: handshake failures, breaker trips, fallback activation, parity mismatches (dual), resolve outcomes.
3. Ensure no raw tokens/secrets are logged.
Acceptance:
1. Operators can diagnose MNO failures from `status`, `jobs/status`, and audit stream alone.

### MC-MNO-008 P0 Test Gate
Priority: P0
Files: `crates/carsinos-gateway/tests/e2e_process.rs`, `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Add/extend tests for: handshake failure fallback, breaker behavior, context truncation, and the expanded op coverage.
2. Keep existing `numquam_` and process-level tests green.
Acceptance commands:
1. `cargo check -p carsinos-gateway --bin carsinos-gateway`
2. `cargo test -p carsinos-gateway numquam_ -- --nocapture`
3. `cargo test -p carsinos-gateway numquam_http_integration_wires_context_writeback_and_approval_process_level -- --nocapture`

## P1 Tickets (Operator and Memory Sync)

### MC-MNO-009 Operator Explainability Path (`context.why`)
Priority: P1
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`
Deliver:
1. Add API surface to fetch and return `context.why` explanations for a run’s evidence ids.
2. Persist explanation fetch in audit trail.
Acceptance:
1. Operator can query why specific evidence was used.

### MC-MNO-010 memory.md Sync Pipeline (Local Notes Fallback)
Priority: P1
Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-storage/src/lib.rs`
Deliver:
1. Add scheduled/manual sync job that ingests configured `memory.md` files into local notes/embeddings.
2. Track source path and last sync timestamp.
Acceptance:
1. `memory.md` content becomes searchable via local memory fallback.
2. Sync failures are isolated and do not break run execution.

### MC-MNO-011 Policy Controls for Local-vs-MNO Blend
Priority: P1
Files: `crates/carsinos-protocol/src/lib.rs`, `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Add config knobs for blending behavior (`mno_primary`, `local_fallback_only`, `local_augment`).
2. Expose effective blend mode in run usage metadata.
Acceptance:
1. Blend mode changes are runtime-configurable and test-covered.

## P2 Tickets (Operational Readiness)

### MC-MNO-012 Scheduled Integration Ops Jobs
Priority: P2
Files: `crates/carsinos-gateway/src/main.rs`
Deliver:
1. Add scheduler modes for MNO preflight/health-check and optional parity probe (dual).
2. Emit job-level guardrail reason codes for MNO operations.
Acceptance:
1. Scheduled MNO checks run unattended and fail safely.

### MC-MNO-013 MNO Pipeline Hook Points
Priority: P2
Files: `crates/carsinos-gateway/src/main.rs`, `docs/`
Deliver:
1. Add explicit integration hooks for future MNO import/backfill/eval jobs.
2. Document an operator runbook for those hooks.
Acceptance:
1. Clear upgrade path from “runtime integration only” to memory pipeline orchestration.

### MC-MNO-014 Soak Readiness Checklist
Priority: P2
Files: `docs/`
Deliver:
1. Define soak checklist for memory regressions, fallback rates, breaker trip rates, and writeback queue behavior.
2. Add rollback steps for MNO outage or contract drift.
Acceptance:
1. Team can run a multi-day soak with clear pass/fail criteria and rollback plan.

## Execution Guardrails for AppDex
1. Do not start any P1 ticket until all P0 acceptance commands are green.
2. Each ticket requires tests in the same PR.
3. No ticket may increase run-loop fragility; fallback path must remain operational.
4. Keep each PR small and independently releasable.
