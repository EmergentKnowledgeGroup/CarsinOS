# MNO carsinOS Assistant Memory Integration Blockerboard

Date: 2026-03-12
Source spec: `docs/MNO_CARSINOS_ASSISTANT_MEMORY_INTEGRATION_SPEC.md`
Scope: Phase 0 + Phase 1
Current status: blockers B1-B10 resolved locally on 2026-03-12

## Resolution Snapshot

- B1-B4 resolved in protocol/gateway with assistant-bound binding, routing, capability probing, and wrapper surfaces.
- B5-B7 resolved in Mission Control with additive Memory tab wiring, isolated controller/cache behavior, and server-authoritative graph views.
- B8-B10 resolved through explicit unavailable/unauthorized UX and green backend/frontend/e2e regression coverage.

## Critical Path

1. Assistant-bound binding model in protocol/gateway
2. `integration-v1` routing by assistant
3. native MNO read wrappers in gateway
4. Memory tab controller + additive tab wiring
5. Memory tab read UX
6. isolation/regression tests

## Blockers

### B1. Assistant Binding Model

- Blocks: all backend and frontend work
- Depends on: none
- Owner area: protocol + gateway
- Definition: carsinOS has no canonical assistant -> MNO binding model yet
- Done when: one assistant resolves to zero or one active MNO binding with explicit status and surface availability

### B2. Safe Gateway Routing

- Blocks: orchestration correctness, read wrappers, tests
- Depends on: B1
- Owner area: gateway
- Definition: current Numquam integration is global, not assistant-bound
- Done when: every orchestration call resolves through the selected assistant lane only

### B3. Native Surface Availability Probe

- Blocks: resilient Memory tab rendering
- Depends on: B1, B2
- Owner area: gateway
- Definition: Memory tab cannot safely assume every native MNO endpoint exists
- Done when: gateway publishes per-surface availability and unsupported endpoints degrade non-fatally

### B4. Gateway Wrapper Surface

- Blocks: Mission Control Memory tab
- Depends on: B2, B3
- Owner area: gateway + protocol
- Definition: frontend cannot call MNO directly in v1
- Done when: cards, episodes, graph, why, citations, health, telemetry, and decision-reason reads exist through gateway wrappers

### B5. Memory Tab Additive Wiring

- Blocks: visible product delivery
- Depends on: B4
- Owner area: Mission Control app shell
- Definition: Memory tab does not exist yet
- Done when: Memory is registered in tabs, rendered in `AppContent`, and respects feature gating without disrupting current UX

### B6. Memory Tab State Isolation

- Blocks: trustworthy UX
- Depends on: B5
- Owner area: Mission Control feature/controller
- Definition: stale responses and shared cache keys could leak data across assistants
- Done when: cache keys and in-flight requests are partitioned by `binding_id`, and stale responses are discarded on assistant changes

### B7. Graph Authority

- Blocks: graph UX correctness
- Depends on: B4, B6
- Owner area: gateway + Memory UI
- Definition: carsinOS must not synthesize graph neighborhoods client-side
- Done when: `graph-map` drives overview, `graph/neighbors` drives drilldown, and truncation is shown exactly as returned

### B8. Read Authorization

- Blocks: secure ship
- Depends on: B4, B5
- Owner area: gateway + Mission Control
- Definition: read access rules are currently implicit
- Done when: read surfaces require explicit auth/capability checks and unauthorized UX is explicit

### B9. Adjacent Surface Consistency

- Blocks: rollout safety
- Depends on: B5, B6
- Owner area: Cockpit + assistant surfaces
- Definition: derived Memory signals can drift from per-assistant kill switches
- Done when: disabling a lane hides/locks all Memory-derived surfaces consistently

### B10. Isolation Regression Coverage

- Blocks: PR readiness
- Depends on: B2, B4, B6, B7, B8
- Owner area: backend + frontend test suites
- Definition: no current proof that assistants cannot leak data across lanes
- Done when: backend, frontend, and e2e suites prove no-leak behavior and mismatch/degrade cases

## Parallelizable Lanes

### Lane A

- B1 assistant binding model
- B2 safe gateway routing
- B3 native surface availability probe

### Lane B

- B5 additive Memory tab wiring
- basic empty/unconfigured UI states once B1 types are settled

### Lane C

- B10 test harness preparation
- mock data / API test scaffolding for wrappers and Memory tab states

## Known Collision Risks

- `apps/mission-control/src/app/*` is already dirty in the worktree
- `apps/mission-control/src/lib/api.ts` and `src/types.ts` are already dirty
- `crates/carsinos-gateway/src/main.rs` is already dirty
- `crates/carsinos-protocol/src/lib.rs` is already dirty

Work must layer on top of existing changes and avoid reverting unrelated edits.

## Backout Levers

- global Memory feature flag
- per-assistant lane disable
- hide Cockpit/assistant Memory-derived surfaces when a lane is disabled
- keep `integration-v1` orchestration intact even if Memory UI is rolled back

## Stop-Ship Triggers

- cross-assistant data exposure
- shared/default MNO fallback across assistants
- unauthorized Memory-tab read exposure
