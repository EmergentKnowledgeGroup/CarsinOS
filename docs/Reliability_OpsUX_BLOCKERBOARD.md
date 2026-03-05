# Reliability + OpsUX Blockerboard

Date: 2026-03-05  
Source spec: [Reliability_OpsUX_upgrade.md](./Reliability_OpsUX_upgrade.md)
Tracking note: The source spec references this blockerboard directly for execution control.

## Purpose

Execution control board for the Reliability + Ops UX program.  
Use this board to track sequencing, blockers, and phase acceptance criteria.

## Status Legend

- `TODO`: defined, not started
- `IN_PROGRESS`: actively being implemented
- `BLOCKED`: cannot proceed due to dependency/blocker
- `VERIFYING`: implemented, in test/validation
- `DONE`: merged and accepted

## Global Blockers Register

| Blocker ID | Owner | Status | Blocker | Impact | Unblocks |
| --- | --- | --- | --- | --- | --- |
| BLK-01 | QA/Automation + Gateway | DONE | Deterministic stub gateway + websocket harness finalized (Playwright core E2E wired to deterministic local gateway/ws) | Stable E2E foundation available | P2-07, P3-01, P3-02, P3-03 |
| BLK-02 | Platform/CI | DONE | Desktop (Tauri) CI runner/smoke pipeline finalized (macOS desktop release gate with tauri smoke visual + tauri build sanity) | Release-profile desktop confidence available | P3-04 |
| BLK-03 | Gateway Config + Mission Control Frontend | DONE | Runtime feature-flag controls and kill-switch wiring finalized and validated in quality-gate profiles | Safe incremental rollout controls now available | P2-01, P2-06, P4-02 |
| BLK-04 | Storage | DONE | Session-level durable recovery-log path finalized and validated for soft-clear/restore behavior | `Clear`/undo contract no longer blocked | P2-04 |
| BLK-05 | Gateway Usage API | DONE | Usage/cost contract exposed via gateway read-model route and validated with Phase-4 acceptance/e2e coverage | Optional cost/token module is now unblocked | P4-01, P4-02, P4-03, P4-04 |

## Phase 1 (Foundations + Crash-Proofing + Core Gate)

Phase objective: quality gate and crash-recovery baseline are production safe.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1-01 | Mission Control Frontend | Implement `quality:gate` entrypoint with `pr` and `release` profiles | DONE | - | - | A1 | Local and CI gate pass/fail behavior matches profile definitions |
| P1-02 | QA/Automation + Gateway | Build deterministic stub gateway + websocket harness for E2E | DONE | P1-01 | - | A3, Sec7(P1 test stability) | Repeatable E2E runs without external providers |
| P1-03 | Mission Control Frontend | Add unit tests for runtime URL/token, board logic, WS parsing/reconnect, summarization, redaction | DONE | P1-01 | - | A2 | Required unit suites green in gate |
| P1-04 | QA/Automation | Add core E2E set: onboarding, connect/baseline, controlled crash recovery | DONE | P1-02 | - | A3(Phase 1) | E2E scenarios deterministic and green |
| P1-05 | Mission Control Frontend | Implement per-tab boundary, global boundary, fallback loop guard + operator recovery actions | DONE | P1-03 | - | C2, C3, C4 | Forced crashes recover without full app loss |
| P1-06 | Mission Control Frontend | Implement and verify secret redaction for copy/debug surfaces | DONE | P1-05 | - | C3 redaction, Sec7(P1 security) | Copy/debug output scrubs required secret classes |
| P1-07 | Platform/CI | Add release-profile Tauri smoke subset + desktop build sanity gate | DONE | P1-01 | - | A1, A3, A4 | Release profile includes desktop sanity and smoke checks |
| P1-08 | QA/Automation | Publish phase-scoped acceptance matrix (Section 7 bullet -> automated assertion) | DONE | P1-03, P1-04, P1-05, P1-06 | - | A4, Sec7 | Every P1-tagged checklist bullet has linked assertion(s) |

## Phase 2 (Live Feed v1 + Incident Behavior + Burst Safety)

Phase objective: always-accessible live operational feed with safe behavior under load.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P2-01 | Mission Control Frontend | Add right-side Live Feed drawer accessible from every tab | DONE | P1-05 | - | B1, B5 | Drawer reachable globally without tab disruption |
| P2-02 | Mission Control Frontend + Gateway | Implement event normalization adapter/envelope handling with safe fallback defaults | DONE | P2-01 | - | B1.1 | Incomplete metadata degrades safely (`other/normal`) |
| P2-03 | Mission Control Frontend | Implement filters, unread badge, pause semantics, mark-read behavior | DONE | P2-01, P2-02 | - | B2, B3 | Unread/pause/filter behavior matches spec |
| P2-04 | Mission Control Frontend + Storage | Implement soft clear + undo windows + 30-min recoverability via persisted session recovery log | DONE | P2-03 | - | B3, B4 recoverability contract | Recoverability promise holds under burst and cap conditions |
| P2-05 | Mission Control Frontend | Add virtualization, bounded in-memory caps, overflow policy, and interaction SLO instrumentation | DONE | P2-02 | - | B4 | Burst tests meet cap and responsiveness constraints |
| P2-06 | Mission Control Frontend + Gateway Config | Implement incident mode auto/manual rules, cooldown, and override precedence | DONE | P2-03 | - | B3 incident mode | Incident transitions follow rule matrix |
| P2-07 | QA/Automation | Add Live Feed E2E + burst/overflow + reconnect flap coverage | DONE | P2-04, P2-05, P2-06 | - | B5, Sec7(P2) | All P2-tagged checklist assertions green |

## Phase 3 (Operator Workflow Expansion)

Phase objective: end-to-end confidence for real operator workflows.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P3-01 | QA/Automation | Expand E2E boards workflow (create/move/run/persist/refresh) | DONE | P1-02, P2-07 | - | A3(Phase 3) | Boards flows deterministic and green |
| P3-02 | QA/Automation | Expand E2E focus approvals workflow (approve/deny/state updates/counts) | DONE | P1-02, P2-07 | - | A3(Phase 3) | Focus approvals workflows deterministic and green |
| P3-03 | QA/Automation | Add reconnect-edge suite (rapid flap, malformed events, state consistency) | DONE | P2-05, P2-07 | - | Sec7 reliability | No duplicated/exploded state under flap scenarios |
| P3-04 | Platform/CI | Validate Tauri parity for representative workflow scenarios | DONE | P1-07 | - | A3 release/Tauri parity intent | Desktop smoke parity evidence recorded in [`mission-control_p3_tauri_parity_evidence.md`](./mission-control_p3_tauri_parity_evidence.md) |
| P3-05 | QA/Automation | Complete P3 acceptance mapping for Section 7 scoped bullets | DONE | P3-01, P3-02, P3-03, P3-04 | - | Sec7 acceptance binding | P3 checklist matrix complete and green (`docs/mission-control_p3_acceptance_matrix.json`) |

## Phase 4 (Optional: Cost + Token Visibility)

Phase objective: usage transparency only when safe gateway contract exists.

| Task ID | Owner | Task | Status | Depends On | Blocker | Spec Mapping | Exit Criteria |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P4-01 | Gateway Usage API + Product/Ops | Validate gateway usage contract readiness and field integrity | DONE | - | - | D1 | Contract completeness confirmed via `/api/v1/mission-control/usage` contract fields and gateway tests |
| P4-02 | Mission Control Frontend + Gateway Config | Build summary + breakdown + freshness/staleness states | DONE | P4-01 | - | D2, D3 | Usage UI renders summary/breakdown + explicit freshness/correlation states without misleading claims |
| P4-03 | Mission Control Frontend + Product/Ops | Add optional budget thresholds and non-spam warning behavior | DONE | P4-02 | - | D3 | Threshold warnings are surfaced only when warning/critical ratios are exceeded |
| P4-04 | QA/Automation | Add tests for available-data, unavailable-data, and missing optional correlation slices | DONE | P4-02 | - | D4 | Automated assertions cover available/unavailable/missing-optional permutations (`e2e/p4-usage.spec.ts`) |

## Critical Path (Current)

1. Phase 1-4 execution path complete; no active blockers.

## Definition of Done Per Phase

- Phase 1 DoD: P1 tasks complete and all Section 7 bullets tagged `[P1]` mapped to automated assertions.
- Phase 2 DoD: P2 tasks complete and all Section 7 bullets tagged `[P2]` mapped to automated assertions.
- Phase 3 DoD: P3 tasks complete and all carried-over open items resolved.
- Phase 4 DoD: P4 tasks complete only if BLK-05 is resolved; otherwise feature remains explicitly disabled with clear UX.

## Operating Rules

- Do not start a downstream task while its dependency chain is unresolved.
- Any `BLOCKED` task must reference a blocker ID and an owner in PR/task metadata.
- Update this board whenever a task changes state.
- If emergency bypass is used, log incident ticket and schedule gate rerun within 24 hours (per source spec).
