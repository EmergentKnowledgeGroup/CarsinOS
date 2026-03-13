# carsinOS Unified Connector Registry Blockerboard

Date: 2026-03-12
Source spec: `docs/CARSINOS_UNIFIED_CONNECTOR_REGISTRY_SPEC.md`
Execution checklist: `docs/CARSINOS_UNIFIED_CONNECTOR_REGISTRY_EXECUTION_CHECKLIST.md`
Current status: planned from locked spec

## Purpose

Execution control board for the unified connector registry buildout.

Use this board to track dependencies, parallel lanes, rollout hazards, and validation readiness before implementation starts.

## Status Legend

- `TODO`: defined, not started
- `IN_PROGRESS`: actively being implemented
- `BLOCKED`: cannot proceed due to dependency/blocker
- `VERIFYING`: implemented and in validation
- `DONE`: merged and accepted

## Global Blockers Register

| Blocker ID | Owner | Status | Blocker | Impact | Unblocks |
| --- | --- | --- | --- | --- | --- |
| BLK-01 | Protocol + Storage | TODO | Canonical connector object model, migrations, and state ownership are not landed yet | Gateway and frontend will drift if they build against unstable state semantics | P0-03, P1-01, P1-02, P1-03 |
| BLK-02 | Gateway/API | TODO | Connector contract routes, import safety, and live-version preservation rules are not implemented yet | No safe frontend wiring or runtime exposure is possible | P1-04, P1-05, P1-06, P1-07 |
| BLK-03 | Conversion Pipeline | TODO | MCP/OpenAPI/GraphQL normalization, naming stability, and write classification logic are not implemented yet | Review/publish semantics and runtime registration cannot be trusted | P1-05, P1-06, P1-07 |
| BLK-04 | Runtime Integration | TODO | Connector-derived tools are not registered through the existing runtime/audit/approval path yet | Tool discovery could diverge from actual execution behavior | P1-07, P1-08 |
| BLK-05 | Mission Control App Shell | TODO | `Connectors` tab seam, rollout guard, and API-availability fallback are not finalized | Frontend cannot ship the new surface safely | P0-01, P1-08, P1-09 |
| BLK-06 | Auth + Interaction Layer | TODO | Shared auth, per-agent override, durable interactions, and token invalidation are not implemented | Connector auth will be brittle, unsafe, or unauditable | P1-06, P1-08, P1-09 |
| BLK-07 | QA/Validation | TODO | Required web, Rust, and e2e gates are not green on the integrated change set | PR cannot open under required flow | P1-10 |

## Phase 0 (Framing + Scaffolding)

Phase objective: establish the connector registry shell, rollout guard, and canonical ownership without exposing live connector behavior prematurely.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P0-01 | Mission Control App Shell | Register `Connectors` tab and add rollout/API-availability guard | TODO | - | BLK-05 | Sec9, Sec14 | Missing endpoints hide or degrade the tab cleanly |
| P0-02 | Protocol + Storage | Land connector object model and canonical state ownership | TODO | - | BLK-01 | Sec5, Sec6 | No duplicate state authority remains in the contracts |
| P0-03 | Gateway/API | Add connector metadata to existing tool capability/read contracts without enabling imports yet | TODO | P0-02 | BLK-02 | Sec11, Sec13 | Existing tool discovery can carry connector-origin data safely |

## Phase 1A (Persistence + Gateway Core)

Phase objective: make connector import, versioning, and safe lifecycle control real on the backend.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-01 | Storage | Add source/version/conversion/published-tool/assignment/auth/interaction persistence and migrations | TODO | P0-02 | BLK-01 | Sec6, Sec13.1 | Connector lifecycle data persists safely |
| P1-02 | Gateway/API | Add catalog/install/import/export/conversion/diff/publish/unpublish/rollback routes | TODO | P1-01 | BLK-02 | Sec7, Sec11 | Gateway exposes the full registry control surface |
| P1-03 | Gateway/API | Enforce SSRF, digest, external-reference, trust-state, and live-version preservation rules | TODO | P1-01, P1-02 | BLK-02 | Sec5.8, Sec5.9, Sec7.1 | Unsafe imports and failed updates fail closed |

## Phase 1B (Conversion + Runtime Reuse)

Phase objective: normalize outside connectors into safe reviewed carsinOS tools and execute them through the existing runtime.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-04 | Conversion Pipeline | Implement MCP/OpenAPI/GraphQL adapters with deterministic ids/names and explicit write classification | TODO | P1-01 | BLK-03 | Sec5.11, Sec7.2, Sec13.3 | Every proposal is classifiable, reviewable, and stable |
| P1-05 | Conversion Pipeline + Gateway | Implement review selection, alias overrides, and publish/supersede rules | TODO | P1-02, P1-04 | BLK-03 | Sec7.3, Sec7.4, Sec8.1 | Only reviewed selected operations can publish |
| P1-06 | Runtime Integration | Register connector-derived tools into the existing runtime with approval/audit/policy reuse | TODO | P1-02, P1-05 | BLK-04 | Sec8, Sec13.2 | Connector tools discover and execute through the existing tool path |

## Phase 1C (Mission Control Surface)

Phase objective: expose the registry and workflow cleanly in Mission Control and adjacent read-only surfaces.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-07 | Mission Control Frontend | Build `Connectors` surface with Catalog, Installed, Review, Auth + Interactions, and Health | TODO | P0-01, P1-02, P1-05 | BLK-05 | Sec9, Sec13.4 | Operator can manage the full connector lifecycle in-app |
| P1-08 | Mission Control Frontend | Add `Extensions` mirror and connector-origin visibility in adjacent surfaces | TODO | P1-06, P1-07 | BLK-04, BLK-05 | Sec5.6, Sec9.3, Sec13.5 | Adjacent surfaces gain consistent read-only connector context |
| P1-09 | Mission Control Frontend + Gateway | Implement shared auth, per-agent override, durable interactions, and resume/cancel flows | TODO | P1-02, P1-06, P1-07 | BLK-06 | Sec5.7, Sec6.7, Sec6.8, Sec8.4 | Auth and interaction flows are durable and auditable |

## Phase 1D (Validation + PR Flow)

Phase objective: prove the connector layer works on its own merits and ship through the required PR process.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-10 | QA/Automation | Run required Rust, Mission Control, and e2e gates and write post-green checkpoint | TODO | P1-03, P1-06, P1-07, P1-08, P1-09 | BLK-07 | Checklist Batch 8 | Local gates are green and residual risks are documented |
| P1-11 | PR Owner | Open PR, request `@coderabbitai review`, wait, address findings, rerun gates, merge, and write required checkpoints | TODO | P1-10 | BLK-07 | Checklist PR Flow | PR is merged only after review completion and revalidation |

## Critical Path

1. `P0-02`
2. `P1-01`
3. `P1-02` -> `P1-03`
4. `P1-04` -> `P1-05`
5. `P1-06`
6. `P1-07`
7. `P1-08` + `P1-09`
8. `P1-10`
9. `P1-11`

## Parallel Work Lanes

- After `P0-02`, `P0-01` and `P1-01` can proceed in parallel.
- After `P1-02`, backend import-safety work in `P1-03` can run in parallel with adapter work in `P1-04`.
- After `P1-06`, `P1-08` and `P1-09` can proceed in parallel if shared API/type changes are coordinated.
- Test harness prep can begin early so connector e2e coverage does not become a late blocker.

## Known Collision Risks

- `crates/carsinos-gateway/src/main.rs` is already an active integration surface and likely needs extraction to avoid concentration debt.
- `crates/carsinos-storage/src/lib.rs` and `crates/carsinos-protocol/src/lib.rs` are already common collision points.
- `apps/mission-control/src/app/*`, `src/lib/api.ts`, and `src/types.ts` are shared frontend seams and must be edited carefully on top of existing worktree changes.

## Backout Levers

- global `Connectors` rollout flag
- gateway route disable that degrades read paths cleanly
- hide `Connectors` tab if endpoints are unavailable
- preserve connector records and prior reviewed live versions even if the UI is rolled back

## Stop-Ship Triggers

- secret or live-token exposure in UI, logs, exports, or checkpoints
- any path that publishes or executes `unsafe_blocked` operations
- any silent mutation of live tools during update
- any ability for disabled or unassigned connectors to appear callable
- any bypass of existing approval, policy, or audit controls
