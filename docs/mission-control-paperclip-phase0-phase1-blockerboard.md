# Mission Control Paperclip Phase 0-1 Blockerboard

Date: 2026-03-09  
Source spec: [mission-control-paperclip-phase0-phase1-plan.md](./mission-control-paperclip-phase0-phase1-plan.md)  
Execution checklist: [mission-control-paperclip-phase0-phase1-execution-checklist.md](./mission-control-paperclip-phase0-phase1-execution-checklist.md)

## Purpose

Execution control board for the Mission Control Phase 0 and Phase 1 management-layer buildout.

Use this board to track sequencing, dependencies, blockers, validation readiness, and merge readiness.

## Status Legend

- `TODO`: defined, not started
- `IN_PROGRESS`: actively being implemented
- `BLOCKED`: cannot proceed due to dependency/blocker
- `VERIFYING`: implemented, in test/validation
- `DONE`: merged and accepted

## Global Blockers Register

| Blocker ID | Owner | Status | Blocker | Impact | Unblocks |
| --- | --- | --- | --- | --- | --- |
| BLK-01 | Protocol + Storage | TODO | Canonical management contracts and persistence rules are finalized for goals, projects, tasks, links, hierarchy, and presets | No safe gateway/API work until contracts are stable | P1-03, P1-04, P1-05 |
| BLK-02 | Gateway/API | TODO | Gateway CRUD, filter/pagination, and strategy summary read model are finalized | Mission Control cannot wire real data or validate UX behavior | P1-06, P1-07, P1-08, P1-09 |
| BLK-03 | Mission Control App Shell | TODO | `Strategy` tab registration, rollout flag seam, and partial-availability behavior are finalized | Frontend cannot integrate the new surface safely | P0-02, P1-06, P1-07 |
| BLK-04 | Cockpit + Team + Onboarding | TODO | Cross-surface integration seams for widgets, hierarchy editing, and preset application are finalized | Existing tab integration and bootstrap flows remain blocked | P1-08, P1-09 |
| BLK-05 | QA/Validation | TODO | Required web and Rust validation gates are green on the integrated change set | PR cannot open under required flow | P1-10 |

## Phase 0 (Framing + Scaffolding)

Phase objective: add the `Strategy` surface, rollout seams, and copy/IA updates without disrupting current Mission Control operator flow.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P0-01 | Mission Control App Shell | Register `Strategy` tab, preserve nav order, preserve `1-9`, add `0` shortcut, use `Compass` icon | TODO | - | - | Sec2.2, Sec7.1, Sec10 Task 0.1 | `Strategy` appears after `Cockpit` and no existing shortcut regresses |
| P0-02 | Mission Control Frontend | Add `strategy_hub` rollout seam and partial-availability fallback behavior | TODO | P0-01 | BLK-03 | Sec4.5, Sec9.1, Sec10 Task 0.2 | Strategy hides cleanly and missing backend support fails soft |
| P0-03 | Mission Control Frontend + Docs/UX | Update guided tour, help, and assistant/onboarding language to establish `Strategy = management`, `Boards = execution` | TODO | P0-01 | - | Sec2.2, Sec10 Task 0.3 | App copy no longer implies boards are the only work model |

## Phase 1A (Protocol + Storage Foundations)

Phase objective: finalize contracts and persistence so the management model is unambiguous and safe to build against.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-01 | Protocol | Add goal/project/task/summary/preset contracts and extended agent shape | TODO | P0-02 | - | Sec4, Sec5.1, Sec5.6, Sec10 Task 1.1 | Protocol types compile and encode timestamp/unit semantics clearly |
| P1-02 | Storage | Add persistence for goals, projects, tasks, presets, and hierarchy references | TODO | P1-01 | BLK-01 | Sec5, Sec10 Task 1.2 | CRUD persistence exists for all new entities |
| P1-03 | Storage | Enforce slug uniqueness, stale-task query, link exclusivity, force reassignment, and hierarchy-cycle/delete guards | TODO | P1-02 | BLK-01 | Sec4, Sec5.1-5.5, Sec10 Task 1.2 | Validation rules are enforced and covered by tests |

## Phase 1B (Gateway + Summary Read Model)

Phase objective: expose the management layer through carsinOS-native API routes and typed summary views.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-04 | Gateway/API | Add CRUD routes for goals, projects, tasks, and presets | TODO | P1-03 | BLK-01 | Sec6.1, Sec6.5 | Routes exist and match contract/update semantics |
| P1-05 | Gateway/API | Add link/unlink routes and agent contract updates | TODO | P1-03 | BLK-01 | Sec5.4, Sec5.5, Sec6.2, Sec6.3 | Linking and agent hierarchy APIs behave deterministically |
| P1-06 | Gateway/API | Add `strategy/summary` route with typed projections, top-N truncation, timezone/day-boundary handling, and unattributed spend | TODO | P1-04, P1-05 | BLK-02 | Sec4.1, Sec6.4 | Strategy summary can drive frontend without guesswork |

## Phase 1C (Strategy Surface + Existing Tab Integration)

Phase objective: expose the new management model in Mission Control without breaking existing operator workflows.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-07 | Mission Control Frontend | Add frontend wrappers/types and build `Strategy` surface (summary strip, navigator, task split view, CRUD, in-app deep-links) | TODO | P1-04, P1-06, P0-02 | BLK-02, BLK-03 | Sec6.4, Sec7.1, Sec10 Task 1.4, Task 1.5 | Operator can create and manage goal -> project -> task entirely in app |
| P1-08 | Mission Control Frontend | Add additive context to Boards, Calendar, Focus, Team, and Cockpit; keep existing behaviors intact | TODO | P1-07 | BLK-04 | Sec7.2, Sec8.3, Sec10 Task 1.6 | Existing surfaces gain context without flow regression |
| P1-09 | Mission Control Frontend | Extend onboarding and Team with bootstrap preset apply/save/import/export flows | TODO | P1-07, P1-08 | BLK-04 | Sec5.6, Sec7.3, Sec10 Task 1.7 | Presets work in current flows and remain setup-default scoped |

## Phase 1D (Validation + PR Flow)

Phase objective: complete required validation and land under repo PR process.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-10 | QA/Automation + Mission Control + Gateway | Run required web and Rust validation gates and write post-green checkpoint | TODO | P1-07, P1-08, P1-09 | BLK-05 | Checklist Batch 6, Sec10 Task 1.8 | `lint`, `typecheck`, `test:unit`, `build`, and targeted `cargo test` are green |
| P1-11 | PR Owner | Open PR, request `@coderabbitai review`, wait for completion, address findings, rerun gates, merge, and write required checkpoints | TODO | P1-10 | BLK-05 | Checklist Batch 7, Sec10 Task 1.9 | PR merged only after CodeRabbit completion and revalidation |

## Critical Path

1. `P0-01` -> `P0-02`
2. `P1-01` -> `P1-02` -> `P1-03`
3. `P1-04` + `P1-05` -> `P1-06`
4. `P1-07`
5. `P1-08` + `P1-09`
6. `P1-10`
7. `P1-11`

## Parallel Work Lanes

- After `P1-03`, `P1-04` and `P1-05` can run in parallel.
- After `P1-07`, `P1-08` and `P1-09` can run in parallel if they respect shared API/type changes.
- Docs/copy work from `P0-03` can proceed in parallel with backend work once tab naming and IA are locked.

## Definition of Done

- Phase 0 DoD: `Strategy` shell exists, rollout gating exists, and app language reflects the management-vs-execution split.
- Phase 1A DoD: protocol/storage rules for goals/projects/tasks/links/hierarchy/presets are implemented and tested.
- Phase 1B DoD: gateway routes and summary read model are implemented and typed.
- Phase 1C DoD: Strategy plus cross-surface integration is implemented without operator-flow regressions.
- Phase 1D DoD: required gates are green, CodeRabbit review is completed, findings are handled, and merge/post-merge checkpoints are written.

## Operating Rules

- Do not start downstream tasks before dependencies are resolved.
- Any `BLOCKED` task must reference a blocker ID and responsible owner.
- Update this board whenever task state changes.
- Preserve current user layouts and current runtime behavior unless the folded spec explicitly authorizes the change.
