# MNO carsinOS Assistant Memory Integration Execution Checklist

Date: 2026-03-12
Source spec: `docs/MNO_CARSINOS_ASSISTANT_MEMORY_INTEGRATION_SPEC.md`
Implementation batch: Phase 0 + Phase 1 only
Execution status: completed locally and green on 2026-03-12

## Validation Summary

- `npm run lint` in `apps/mission-control`
- `npm run typecheck` in `apps/mission-control`
- `npm run test:unit` in `apps/mission-control`
- `npm run build` in `apps/mission-control`
- `npx playwright test --config=playwright.config.ts e2e/memory.spec.ts` in `apps/mission-control`
- `npm run test:e2e:full` in `apps/mission-control`
- `python3 scripts/mission_control_quality_gate.py --profile pr`
- `python3 scripts/mission_control_quality_gate.py --profile release --fail-on-blocked`
- `scripts/security_pr_gate.sh`

## 1. Entry Criteria

- `docs/MNO_CARSINOS_ASSISTANT_MEMORY_INTEGRATION_SPEC.md` is the active source of truth.
- Assistant isolation is locked as one MNO sidecar/runtime per assistant binding.
- `integration-v1` is the orchestration contract.
- Gateway-mediated native MNO wrappers are the only allowed frontend path in v1.
- Existing dirty worktree changes are preserved and worked with, not reverted.

## 2. Batch A: Assistant-Bound Backend Binding Model

### Tasks

- add assistant-memory binding model to protocol types/config surfaces
- define binding status and native surface availability types
- define read-role / trusted-operator flags needed by Mission Control
- extend gateway config/runtime resolution so the selected assistant resolves to one MNO binding
- reject untrusted or malformed MNO target configuration

### Exit Criteria

- one assistant has zero or one active MNO binding in carsinOS state
- gateway can resolve the selected assistant’s MNO lane deterministically
- no shared/default MNO fallback remains possible

## 3. Batch B: `integration-v1` Assistant Routing + Status

### Tasks

- route `context.build`, `context.why`, `writeback.propose`, `writeback.resolve`, `health.get`, and `capabilities.get` through the selected assistant binding
- preserve `request_id`, `session_id`, `run_id`, and assistant correlation
- define orchestration-health precedence over native runtime health
- implement degrade / unavailable / unauthorized state handling
- add capability probing for required native surfaces and publish per-surface availability

### Exit Criteria

- orchestration requests always route to the selected assistant MNO lane
- degraded/unavailable/unauthorized states are explicit and testable
- missing native surfaces downgrade non-fatally

## 4. Batch C: Native MNO Read Wrappers In Gateway

### Tasks

- add gateway-mediated wrappers/read models for:
  - cards
  - card detail / atom detail
  - graph overview
  - graph neighbors
  - episodes
  - turn why
  - citation lookup
  - runtime health
  - telemetry summary
  - telemetry turns
  - decision reasons
- enforce bounded query semantics for cards, episodes, telemetry, and graph neighbors
- preserve authoritative graph/truncation semantics from MNO
- scope every wrapper server-side to the current assistant binding

### Exit Criteria

- Mission Control has a consistent gateway API surface for all Phase 1 reads
- no direct frontend-to-MNO calls exist
- graph-map and graph-neighbors are the only graph canvas sources

## 5. Batch D: Mission Control Memory Tab Shell

### Tasks

- add `Memory` to the Mission Control tab model
- wire the tab into App shell/content/controller without disrupting existing flow
- add feature gating for the Memory surface
- add empty, unconfigured, unauthorized, degraded, and unsupported-surface states

### Exit Criteria

- Memory tab is additive and stable in nav/shortcut flow
- disabled or unsupported Memory lanes do not break the rest of Mission Control

## 6. Batch E: Memory Tab Read UX

### Tasks

- build assistant-scoped Memory controller
- build assistant identity/status strip
- build cards/episodes browse pane
- build graph overview + drilldown workspace
- build detail/explainability panel
- build citation inspector
- surface orchestration/native health mismatch warning
- partition cache and in-flight requests by `binding_id`
- discard stale responses when assistant binding changes

### Exit Criteria

- operator can inspect the selected assistant’s cards, episodes, graph, why, citations, and telemetry
- no cross-assistant state bleed occurs on assistant switching
- graph truncation/limits are surfaced honestly

## 7. Batch F: Additive Adjacent Surfaces

### Tasks

- add assistant-bound MNO health context to Cockpit
- add assistant/run detail hooks for explainability where appropriate
- keep all adjacent surfaces behind the same per-assistant availability rules

### Exit Criteria

- Memory-derived status outside the Memory tab is additive only
- disabling a lane hides or locks every derived surface consistently

## 8. Batch G: Tests

### Backend

- per-assistant binding selection
- correct `integration-v1` routing by assistant
- no cross-assistant fallback
- native read wrapper coverage
- capability probe downgrade behavior
- auth/unauthorized behavior
- base URL validation / proxy safety

### Frontend

- Memory tab empty/unconfigured state
- unauthorized / degraded / unsupported state rendering
- cache partitioning by `binding_id`
- stale-response discard after assistant switch
- graph overview + drilldown rendering

### End-to-end

- Assistant A and Assistant B never leak memory data across lanes
- orchestration/native health mismatch is visible
- unsupported native endpoints degrade non-fatally
- current Mission Control flow does not regress

## 9. Required Gates Before PR

- `npm run typecheck`
- `npm run lint`
- `npm run test:unit`
- `npm run build`
- targeted Mission Control e2e for Memory tab + assistant routing
- targeted Rust tests for gateway/protocol integration changes

## 10. Explicitly Deferred From This Batch

- proposal inbox and mutation actions
- episode enable/disable/edit actions
- direct frontend-to-MNO paths
- shared multi-assistant MNO runtime/service design
- standalone MNO UI parity work

## 11. Stop-Ship Conditions

- any cross-assistant data exposure
- any shared/default MNO fallback across assistants
- any unauthorized Memory-tab read exposure
- any client-side graph traversal that diverges from MNO authority

## 12. PR Flow Reminder

1. local gates green
2. open PR
3. request CodeRabbit review
4. wait for CodeRabbit completion
5. address findings and rerun gates
6. merge only after review is complete
