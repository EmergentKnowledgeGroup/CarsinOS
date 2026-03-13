# carsinOS Unified Connector Registry Execution Checklist

Date: 2026-03-12
Source spec: `docs/CARSINOS_UNIFIED_CONNECTOR_REGISTRY_SPEC.md`
Execution status: planned from locked spec

## Purpose

Execution checklist for building the unified connector registry and agentic workflow layer without introducing a second runtime, silent tool mutation, or connector-specific prompt glue.

Use this checklist to drive implementation order, validation, and PR discipline.

## Operating Rules

1. `Connectors` is the canonical owner; `Extensions` is read-only mirror only.
2. No direct frontend-to-connector execution path is allowed.
3. Outside imports must follow `import -> convert -> review -> enable`.
4. `unsafe_blocked` operations must never publish.
5. Connector updates must preserve the current live version until re-review and explicit re-enable succeed.
6. Shared auth is the default; per-agent override wins when present.
7. UI work must use the required frontend-design workflow and phase checkpoints.
8. Update `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json` at phase start, post-green tests, PR open, and post-merge.
9. Before each major edit block and after each meaningful milestone, run the context checkpoint snapshot command.
10. Do not merge before `@coderabbitai review` completes and findings are handled.

## Entry Criteria

- `docs/CARSINOS_UNIFIED_CONNECTOR_REGISTRY_SPEC.md` is the active source of truth.
- Existing dirty worktree changes outside this scope are identified and left untouched.
- The implementation owner understands that carsinOS remains the only runtime/control plane.
- The rollout path is feature-gated so Mission Control can hide or degrade the `Connectors` surface if backend endpoints are unavailable.

## Batch 0: Execution Prep

### Tasks

- verify working branch/head and current dirty worktree
- write implementation start checkpoint
- re-read the locked spec and this checklist before code edits
- confirm current tool capability, plugin, and Mission Control tab seams in repo
- confirm which frontend surfaces will require additive connector-origin badges only

### Exit Criteria

- implementation starts from a fresh checkpoint
- repo seams for protocol, storage, gateway, and Mission Control are confirmed

## Batch 1: Protocol and Storage Foundation

### Tasks

- add canonical connector protocol objects:
  - `ConnectorCatalogItem`
  - `ConnectorSource`
  - `ConnectorVersion`
  - `ConnectorConversion`
  - `ConnectorPublishedTool`
  - `ConnectorAssignment`
  - `ConnectorAuthBinding`
  - `ConnectorInteraction`
- encode canonical state ownership:
  - `ConnectorSource.status`
  - `ConnectorSource.current_version_id`
  - `ConnectorConversion.status`
- add migrations and storage helpers for sources, versions, conversions, published tools, assignments, auth bindings, and interactions
- add trust-state persistence and auditable state transitions
- add rollback-safe live-version behavior
- add `external_reference_policy` handling and persistence
- add resume-token expiry/one-time-use persistence
- add storage tests for:
  - state transitions
  - live-version preservation on failed conversion
  - assignment/auth precedence
  - token expiry/invalidation
  - publish/unpublish/supersede behavior

### Exit Criteria

- protocol surface compiles with no duplicate source-of-truth fields
- storage layer can persist connector lifecycle safely and roll back to prior reviewed live versions

## Batch 2: Gateway Contracts and Registry Core

### Tasks

- add connector-oriented routes for:
  - catalog list/detail
  - installed connector list/detail
  - import
  - export
  - conversion run/detail
  - diff review
  - publish/unpublish
  - rollback/re-enable prior reviewed version
  - assignment CRUD
  - auth/session CRUD
  - interaction list/detail/resume/cancel
  - health
- extend tool capability routes with connector-origin metadata, write classification, trust state, and live version context
- enforce URL import SSRF, allow/deny, size, digest, and external-reference rules
- enforce that failed conversion/review does not mutate the current live version
- enforce that disabled connectors block new executions while allowing in-flight executions to complete with audit markers
- expose feature/readiness signal so Mission Control can fail soft when endpoints are unavailable

### Exit Criteria

- gateway contracts match the spec exactly
- connector routes degrade cleanly when disabled or partially unavailable

## Batch 3: Conversion and Publish Pipeline

### Tasks

- implement MCP conversion adapter
- implement OpenAPI conversion adapter
- implement GraphQL conversion adapter
- normalize tool ids/names deterministically
- fail conversion into review when name collisions exist without explicit alias override
- classify every proposed operation into:
  - `read_only`
  - `operator_write_gated`
  - `destructive_write_gated`
  - `unsafe_blocked`
- store warnings, unsupported operations, and diff metadata
- implement review selection with default `select none`
- persist alias overrides so future conversions remain deterministic
- publish only reviewed selected operations
- ensure `unsafe_blocked` proposals cannot publish even if selected

### Exit Criteria

- all supported source kinds convert into stable, reviewable, non-auto-live tool candidates
- publish pipeline preserves safety and determinism across updates

## Batch 4: Runtime Integration

### Tasks

- register connector-derived tools into the existing runtime/tool registry
- reuse existing approval, audit, breaker, and policy paths
- enforce write classification at runtime, not metadata-only
- ensure discovery only exposes tools from:
  - enabled connectors
  - current live versions
  - active assignments
  - policy-allowed scopes
- block queued approvals from executing against disabled connectors
- keep historical published records auditable even when superseded or unpublished

### Exit Criteria

- connector-backed tools execute through the same runtime path as existing tools
- no connector path bypasses existing safety or audit machinery

## Batch 5: Mission Control `Connectors` Surface

### Tasks

- add `Connectors` tab metadata and rollout guard
- wire the tab into Mission Control app shell without interrupting current UX
- add typed frontend API wrappers and connector DTOs
- create `features/connectors` controller boundary
- build `Catalog`
- build `Installed`
- build `Review`
- build `Auth + Interactions`
- build `Health`
- add empty/loading/error/unavailable/degraded states
- ensure secret-safe rendering everywhere
- ensure the tab hides or degrades cleanly if connector endpoints are unavailable

### Exit Criteria

- Mission Control exposes the connector registry cleanly as a first-class additive surface
- frontend never receives raw secrets or direct connector execution handles

## Batch 6: Adjacent Surface Integration

### Tasks

- add read-only generated connector entries to `Extensions`
- add origin badges and deep links from `Extensions` back to `Connectors`
- keep disabled connector entries visible in `Extensions` with disabled/unavailable state and no callable affordance
- add connector-origin metadata to assistant/tool capability surfaces where relevant
- add connector assignment and auth override visibility to `Team`
- add connector health/degraded/auth-required summaries to `Cockpit` only if additive and non-disruptive

### Exit Criteria

- `Connectors` remains canonical while adjacent surfaces gain consistent read-path visibility
- no second editing surface emerges outside `Connectors`

## Batch 7: Durable Auth and Interaction Flow

### Tasks

- implement shared connector auth binding flow
- implement per-agent auth override flow
- enforce precedence of per-agent override over shared auth
- implement durable interaction records for OAuth, auth repair, and structured human input
- enforce one-time-use resume tokens with bounded TTL
- implement resume/cancel/invalidation behavior
- add audit emission for auth and interaction transitions

### Exit Criteria

- auth repair and pause/resume flows are durable, auditable, and scoped correctly
- connector interaction replay is not possible through stale resume tokens

## Batch 8: Tests and Validation

### Backend

- import MCP/OpenAPI/GraphQL sources
- malformed/unsafe import rejection
- SSRF, digest mismatch, and oversize rejection
- collision handling and alias override persistence
- live-version preservation on failed conversion
- publish/unpublish/supersede behavior
- assignment filtering per agent
- shared auth vs per-agent override precedence
- runtime enforcement of write classification
- disable behavior for new executions, queued approvals, and in-flight executions
- audit emission for every connector lifecycle transition

### Frontend

- `Connectors` tab loading, empty, unavailable, degraded, and error states
- catalog/import/review/publish flows
- auth and interaction flows
- secret-safe rendering
- `Extensions` mirror visibility and disabled-state behavior

### End-to-end

- import -> convert -> review -> enable -> assign -> discover -> call
- write-capable tool requires approval/policy path
- connector update produces diff and requires explicit re-enable
- rollback to prior reviewed version works
- per-agent auth override affects only that agent
- disabled connector blocks new calls and visible UI affordances update correctly
- current Mission Control flow does not regress

## Required Gates Before PR

- `npm run lint`
- `npm run typecheck`
- `npm run test:unit`
- `npm run build`
- targeted Mission Control e2e for `Connectors`
- targeted Rust tests for protocol/storage/gateway connector changes
- `python3 scripts/mission_control_quality_gate.py --profile pr`
- `python3 scripts/mission_control_quality_gate.py --profile release --fail-on-blocked`
- `scripts/security_pr_gate.sh`

## Explicitly Deferred

- remote public marketplace service
- tool-level assignment
- auto-live imports
- executor sidecar/runtime adoption
- direct frontend-to-connector execution
- connector-specific custom UIs beyond the unified operator model

## Stop-Ship Conditions

- any path that leaks connector secrets or live auth tokens
- any way to publish or execute an `unsafe_blocked` operation
- any silent mutation of live tools during connector update
- any unassigned or unauthorized agent discovering callable connector tools
- any connector path bypassing existing approval, policy, or audit controls
- any Mission Control hard-failure when connector endpoints are disabled or unavailable

## PR Flow Reminder

1. local gates green
2. open PR
3. request CodeRabbit review
4. wait for CodeRabbit completion
5. address findings and rerun gates
6. merge only after review is complete
