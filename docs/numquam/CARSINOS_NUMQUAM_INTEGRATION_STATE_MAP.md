# carsinOS NumquamOblita Integration State Map

Generated: 2026-03-10

## Purpose

This document captures the current state of NumquamOblita (`MNO`) integration inside carsinOS before the cleaned external MNO folder is handed over for final integration work.

It is a system map, not a proposal.

Important framing:

- this document describes current live integration behavior inside `carsinOS`
- it does not override newer target-architecture specs for per-user routing or lane-scoped MNO topology
- if this file disagrees with the newer routing/memory specs, treat this file as the description of current state, not the desired end-state

## Bottom Line

NumquamOblita integration in carsinOS is backend-real, operationally meaningful, and test-covered.

It is not yet fully productized in Mission Control or GUI surfaces.

The current state is:

- real runtime integration in gateway
- real config and status contract in protocol
- real run-loop usage
- real local-memory fallback and `memory.md` sync
- real approval-driven writeback resolve
- real tests and stubs
- partial operator exposure
- remaining compatibility hardcodes and heuristic defaults

The target direction is now clearer than when this map was written:

- `HTTP integration-v1` remains the primary production contract
- `MCP` remains parity/tooling/diagnostic surface, not the only hot-path dependency
- long-term target topology is one MNO lane per `human_identity + assistant_agent`, not one shared multi-assistant memory core

## Current Live Integration

### Protocol / Contract Surface

carsinOS protocol already defines:

- `RuntimeMemoryConfig`
- `RuntimeNumquamConfig`
- `NumquamIntegrationStatusResponse`
- `RunMemoryWhyRequest` / `RunMemoryWhyResponse`
- `SyncMemorySourcesRequest` / `SyncMemorySourcesResponse`
- `JobStatusResponse.numquam`

Primary file:

- `crates/carsinos-protocol/src/lib.rs`

### Gateway Runtime Integration

Gateway currently implements a concrete `NumquamClient` with:

- transport modes: `http`, `mcp`, `dual`
- canonical envelope parsing
- contract handshake via health + capabilities
- degrade-mode handling
- circuit breaker integration
- request correlation ids
- dual-transport parity merge logic

Primary file:

- `crates/carsinos-gateway/src/main.rs`

### Run-Loop Usage

The active run path uses Numquam in this order:

1. tools
2. `context.build`
3. provider completion
4. `writeback.propose`
5. approval
6. `writeback.resolve`

Current behavior:

- `context.build` is called before provider completion when blend mode permits it.
- local memory is used as fallback or augmentation depending on `memory.blend_mode`.
- `writeback.propose` runs after assistant output.
- `pending_review` writebacks generate carsinOS approvals.
- approving the writeback approval calls upstream `writeback.resolve`.

Primary file:

- `crates/carsinos-gateway/src/main.rs`

### Operational APIs And Job Modes

Current live surfaces include:

- `POST /api/v1/runs/{run_id}/memory/why`
- `POST /api/v1/memory/sync`
- `GET /api/v1/jobs/status`
- scheduler mode `memory.sync`
- scheduler mode `memory.preflight`
- scheduler mode `memory.parity_probe`
- scheduler mode `memory.pipeline.hook`

Primary file:

- `crates/carsinos-gateway/src/main.rs`

### Local Fallback Memory Path

carsinOS also has a local-memory path that is already wired and used alongside / instead of MNO depending on blend mode and degrade state.

This is a statement about current live behavior, not a locked future architecture rule.

In the target lane-scoped model, if MNO is enabled for a lane, MNO should be treated as the long-term memory truth for that lane. Local notes, `memory.md`, and related material remain valuable, but should be treated as source material, adjunct context, or fallback behavior according to policy rather than as a competing assistant-global truth store.

This includes:

- local note sync from configured `memory.md` sources
- note chunking + local embeddings
- local retrieval for provider context fallback / augmentation

Primary file:

- `crates/carsinos-gateway/src/main.rs`

## Adapter Contract carsinOS Currently Expects

### HTTP Surface

Current gateway code expects these HTTP endpoints on the MNO sidecar:

- `GET /api/integration/v1/health`
- `GET /api/integration/v1/capabilities`
- `POST /api/integration/v1/context/build`
- `POST /api/integration/v1/context/why`
- `POST /api/integration/v1/writeback/propose`
- `POST /api/integration/v1/writeback/resolve`

### MCP Surface

Current gateway code expects MCP tools for:

- `integration.health.get`
- `integration.capabilities.get`
- `integration.context.build`
- `integration.context.why`
- `integration.writeback.propose`
- `integration.writeback.resolve`

### Envelope Expectations

carsinOS expects an `integration.v1`-style envelope carrying:

- `schema_version`
- `request_id`
- `request_id_source`
- `operation`
- `ok`
- `degrade_mode`
- `warnings`
- `fallback_recommendation`
- typed `data`
- typed `error`

The current gateway also expects:

- correlation via `session_id` and `run_id`
- principal metadata
- dual-transport parity tolerance
- idempotent writeback proposal behavior

Primary file:

- `crates/carsinos-gateway/src/main.rs`

## What Is Scaffolded Or Partial

### Mission Control / GUI Exposure

MNO is not yet truly surfaced in Mission Control.

What is true today:

- Mission Control fetches `jobs/status`.
- Backend protocol includes `numquam` in status responses.
- Mission Control frontend types currently omit `numquam` from `StatusResponse` and `JobStatusResponse`.
- There are no direct Mission Control views or actions for:
  - MNO health
  - `context.why`
  - `memory.sync`
  - parity probe
  - preflight
  - writeback review visibility beyond generic approvals

Relevant files:

- `apps/mission-control/src/types.ts`
- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/app/useMissionControlController.ts`
- `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx`

### Pipeline Hooks

`memory.pipeline.hook` is present, but it is a hook emitter, not a full pipeline engine.

It currently provides:

- hook name intake
- hook payload passthrough
- event emission
- job output envelope

It does not yet implement actual import/backfill/eval orchestration.

Relevant files:

- `crates/carsinos-gateway/src/main.rs`
- `docs/numquam/MNO_PIPELINE_HOOKS_RUNBOOK.md`

### Soak State

Soak readiness is documented, but this repo map does not itself show a completed long-run MNO promotion report.

Relevant file:

- `docs/numquam/MNO_SOAK_READINESS_CHECKLIST.md`

## What Is Planned / Docs-Driven

The AppDex backlog and checklist describe the MNO track as complete through:

- runtime config hardening
- handshake and health gating
- context policy engine
- context truncation safety
- breaker behavior
- writeback hardening
- explainability API
- `memory.md` sync
- blend policy
- scheduled preflight / parity probe
- pipeline hook points
- soak-readiness documentation

Relevant files:

- `APPDEX_MNO_INTEGRATION_EXECUTABLE_TICKET_BACKLOG.md`
- `CHECKLIST.md`

Pragmatic note:

Most of those items do have real code behind them.
The main delta is not "missing gateway integration."
The delta is "missing full operator/UI exposure and final cleanup of compatibility seams."

## What Is Still Hardcoded Or Compatibility-Based

### Explicit Hardcoded Findings Already Called Out By Audit

The repo already documents three unresolved Numquam-specific hardcoded findings:

1. fallback base URL
2. fallback principal id
3. fallback principal display name

Relevant files:

- `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`
- `docs/CLIDEX_HARDCODE_REMEDIATION_TASK.md`
- `APPDEX_IMPLEMENTATION_TICKET_PACK.md`

### Runtime Compatibility Fallbacks Still In Code

The gateway still falls back to environment and/or literals for:

- `CARSINOS_NUMQUAM_BASE_URL`
- `CARSINOS_NUMQUAM_TOKEN`
- `CARSINOS_NUMQUAM_PRINCIPAL_ID`
- `CARSINOS_NUMQUAM_PRINCIPAL_NAME`
- `CARSINOS_NUMQUAM_MCP_URL`
- hardcoded literal fallback base URL `http://127.0.0.1:7340`
- hardcoded literal fallback principal id `carsinos_gateway`
- hardcoded literal fallback principal display name `carsinOS Gateway`

Primary file:

- `crates/carsinos-gateway/src/main.rs`

### Hardcoded Policy / Heuristic Values

The following are still source-defined heuristics rather than fully operator-configured values:

- MNO policy defaults for `top_k`, `risk_signal`, and message-window sizing
- context budget reservation heuristics
- local memory default `top_k`, candidate counts, max chars, and chunk target size
- local embed model name
- env-based local memory enablement and tuning

Primary file:

- `crates/carsinos-gateway/src/main.rs`

## Current Test And Stub Coverage

The MNO path is well covered in code-level tests.

Current coverage includes:

- unit/integration tests inside gateway for:
  - context + writeback path
  - MCP transport path
  - degrade fallback
  - approval-driven resolve
  - breaker open behavior
  - context truncation
  - `context.why`
- process-level e2e stub coverage
- benchmark coverage with benchmark stub servers

Relevant files:

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-gateway/tests/e2e_process.rs`
- `crates/carsinos-gateway/tests/benchmark_process.rs`

## Practical Gaps Before Clean External Integration

The meaningful gaps are:

1. Mission Control and GUI do not yet expose MNO as a first-class operator surface.
2. Frontend status typing currently drops backend `numquam` fields.
3. `context.why` and `memory.sync` exist as backend APIs but are not yet productized in frontend flows.
4. pipeline hooks are scaffolding only, not full pipeline orchestration.
5. compatibility-era hardcoded/env fallbacks still exist in operational MNO paths.

## Integration Readiness Assessment

If a clean external NumquamOblita folder is handed over next, carsinOS is already prepared to integrate it cleanly at the adapter boundary.

The main work should not be inventing new gateway semantics.

The main work should be:

- verifying the cleaned MNO folder satisfies the current adapter contract
- moving from the current compatibility-era binding model toward lane-scoped `human_identity + assistant_agent` bindings
- reducing compatibility hardcodes where appropriate
- exposing MNO state and actions in Mission Control
- building a proper `Memory` tab / related operator surfaces on top of the already-live backend paths

## Frontend Exposure Implications

The future frontend work is not a greenfield feature.

It should be treated as productizing already-live backend capabilities.

At minimum, a clean `Memory` tab will need:

- frontend types that carry backend `numquam` status through without dropping fields
- API wrappers for `POST /api/v1/runs/{run_id}/memory/why`
- API wrappers for `POST /api/v1/memory/sync`
- health / degrade / parity / last-sync visibility
- explainability views for why context was included
- writeback and approval visibility where memory proposals affect operator review

Likely adjacent frontend surfaces that should also gain MNO exposure:

- `Cockpit` for top-level MNO health and degrade state
- `Jobs` / scheduler views for `memory.sync`, `memory.preflight`, `memory.parity_probe`, and `memory.pipeline.hook`
- `Assistant` or run detail views for `context.why`
- approval surfaces when writeback proposals are pending review

## Recommended Reference Files For Final Integration Pass

- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-gateway/src/main.rs`
- `apps/mission-control/src/types.ts`
- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/app/useMissionControlController.ts`
- `docs/numquam/MNO_PIPELINE_HOOKS_RUNBOOK.md`
- `docs/numquam/MNO_SOAK_READINESS_CHECKLIST.md`
- `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`
- `APPDEX_MNO_INTEGRATION_EXECUTABLE_TICKET_BACKLOG.md`
- `CHECKLIST.md`

## Recommended Next Integration Sequence

1. Validate the cleaned external NumquamOblita folder against the current HTTP, MCP, and envelope contract.
2. Decide whether to keep or simplify `dual` transport behavior for the first clean integration cut.
3. Align runtime topology with the newer lane-scoped `human_identity + assistant_agent` model.
4. Wire Mission Control types and API clients so `numquam` status becomes visible instead of silently dropped.
5. Build the `Memory` tab around the already-live backend actions and health signals.
6. Expose the minimum adjacent status surfaces in `Cockpit`, jobs, approvals, and run explainability views.
7. Remove or reduce the remaining compatibility hardcodes once the external folder is the primary integration target.
