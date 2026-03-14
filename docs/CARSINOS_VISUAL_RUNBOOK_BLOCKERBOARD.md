# carsinOS Visual Runbook Blockerboard

Date: 2026-03-09  
Source spec: [CARSINOS_VISUAL_RUNBOOK_SPEC.md](./CARSINOS_VISUAL_RUNBOOK_SPEC.md)  
Execution checklist: [CARSINOS_VISUAL_RUNBOOK_EXECUTION_CHECKLIST.md](./CARSINOS_VISUAL_RUNBOOK_EXECUTION_CHECKLIST.md)

## Purpose

Execution control board for the visual runbook buildout.

Use this board to track sequencing, dependencies, blockers, validation readiness, and merge readiness.

## Status Legend

- `TODO`: defined, not started
- `IN_PROGRESS`: actively being implemented
- `BLOCKED`: cannot proceed due to dependency/blocker
- `VERIFYING`: implemented, in validation
- `DONE`: merged and accepted

## Global Blockers Register

| Blocker ID | Owner | Status | Blocker | Impact | Unblocks |
| --- | --- | --- | --- | --- | --- |
| RBK-01 | Protocol + Gateway | TODO | Canonical runbook contracts, template catalog, and read-model rules are finalized in code | Frontend cannot build trustworthy UI until state derivation is stable | P1-02, P1-03, P1-04 |
| RBK-02 | Mission Control App Shell | TODO | `runbook` tab registration, `runbook_hub` gating, and refresh/config seams are finalized | Runbook UI cannot land safely without shell and gating support | P2-01, P2-02, P3-01 |
| RBK-03 | Test Harness | TODO | Mock gateway, e2e seams, and backend test coverage are in place for runbook flows | Regression protection is incomplete and local validation will be weak | P2-03, P3-02, P4-01 |
| RBK-04 | Cross-Surface Integration | TODO | Strategy/Boards/Calendar/Focus/Assistant/Cockpit runbook entry seams are finalized | Additive deep links and status chips cannot land cleanly | P3-01, P3-02 |
| RBK-05 | QA / Merge Flow | TODO | Local validation stack is green and PR review flow is ready to start | PR cannot open or merge under repo SOP | P4-01, P4-02 |

## Phase 1: Canonical Model and API

Phase objective: land the derived runbook truth model before any UI depends on it.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-01 | Protocol + Storage | Add runbook protocol types, template catalog, and deterministic selection/state rules | TODO | - | - | Canonical Model, Data Truth, Workflow Templates | Contracts compile and template rules are explicit |
| P1-02 | Gateway | Add runbook list/detail endpoints, cursor behavior, warnings/availability, and feature-disabled handling | TODO | P1-01 | RBK-01 | API and Protocol Contracts, Integrity Rules, Rollout/Backout | Endpoints match spec and fail safely |
| P1-03 | Backend Tests | Add backend coverage for runbook kinds, anchor-only snapshots, cursor stability, mixed facts, and feature gating | TODO | P1-01, P1-02 | RBK-01, RBK-03 | Testing and Quality Gates | Backend truth rules are regression-protected |

## Phase 2: Mission Control Runbook Surface

Phase objective: add the new Runbook tab, config gating, and plain flow UI without disrupting current UX.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P2-01 | Mission Control App Shell | Add `runbook` tab, shortcut behavior, `runbook_hub`, and config wiring | TODO | P1-02 | RBK-02 | Version 1 Scope, Mission Control Integrations, Configuration Rules | Runbook shell exists and hides safely |
| P2-02 | Mission Control Frontend | Build runbook controller, landing page, detail pane, flow renderer, stale handling, and feature-disabled states | TODO | P2-01, P1-02 | RBK-01, RBK-02 | UX Spec, Live Update Rules | Operator can browse truthful runbooks in-app |
| P2-03 | Frontend Tests / E2E | Add runbook tab/controller/unit tests and core e2e coverage for the new surface | TODO | P2-02 | RBK-03 | Testing and Quality Gates | Runbook UI has direct regression coverage |

## Phase 3: Additive Integrations

Phase objective: expose Runbook context and entry points across existing surfaces without creating duplicate renderers.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P3-01 | Mission Control Frontend | Add `Open in Runbook` and status/context chips to Strategy, Boards, Calendar, Focus, and Assistant | TODO | P2-02 | RBK-04 | Mission Control Integrations | Cross-surface entry points work and stay additive |
| P3-02 | Cockpit + E2E | Add cockpit summary integration using shared derivation and verify feature-disabled degradation | TODO | P2-02, P2-03 | RBK-03, RBK-04 | Cockpit Integration, Rollout/Backout | No cockpit-only truth path exists |

## Phase 4: Validation and PR Flow

Phase objective: complete local validation, then run the required PR/review process.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P4-01 | QA / Repo Gates | Run local web, e2e, quality-gate, and security-gate stack; write post-green checkpoint | TODO | P1-03, P2-03, P3-02 | RBK-05 | Execution Checklist Batch 4 | Local gate stack is green |
| P4-02 | PR Owner | Open PR, request CodeRabbit, wait for review, handle findings, rerun gates, merge, and checkpoint | TODO | P4-01 | RBK-05 | Execution Checklist Batch 5 | PR merges only after review completion and revalidation |

## Critical Path

1. `P1-01` -> `P1-02` -> `P1-03`
2. `P2-01` -> `P2-02` -> `P2-03`
3. `P3-01` -> `P3-02`
4. `P4-01`
5. `P4-02`

## Parallel Work Lanes

- After `P1-01`, backend endpoint work and backend test scaffolding can overlap if they share the same truth rules.
- After `P2-01`, runbook UI construction and e2e mock-gateway expansion can overlap.
- After `P2-02`, cross-surface integration work can fan out across Strategy/Boards/Calendar/Focus/Assistant/Cockpit if ownership is kept explicit.

## Definition of Done

- Phase 1 DoD: canonical runbook contracts and backend truth rules are implemented and tested.
- Phase 2 DoD: the Runbook tab is live behind `runbook_hub` with a plain trustworthy UI.
- Phase 3 DoD: additive runbook entry points and summary integrations work across existing surfaces.
- Phase 4 DoD: local gates are green, CodeRabbit review is complete, findings are addressed, and merge checkpoints are written.

## Operating Rules

- Do not start downstream tasks before dependency blockers are cleared.
- Any `BLOCKED` task must reference a blocker ID and responsible owner.
- Update this board whenever task state changes.
- Preserve existing runtime flow unless the spec explicitly authorizes a change.
- Treat hidden background polling, invented progress, and duplicate truth paths as blockers, not cleanup.
