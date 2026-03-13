# CARSINOS_NON_UI_HARDENING_SPINE_SPEC

## Summary

This spec defines the non-UI hardening pass for carsinOS after the recent Strategy, Runbook, Memory, and Connectors integrations.

The goal is to tighten the system spine without changing the visual product surface while active UI/UX audit and cleanup work is still in progress elsewhere.

This hardening pass is engineering-first:

- clarify canonical truth ownership
- reduce backend concentration and cross-domain sprawl
- normalize contracts and config ownership
- harden deterministic validation and runtime safety
- preserve current product behavior except where fixing a confirmed bug

This pass is explicitly not a UI redesign, layout pass, copy pass, or navigation polish pass.

## Locked Decisions

- This pass is non-UI only.
- No intentional UX or wording changes ship as part of this scope.
- No visual redesign, styling cleanup, responsive cleanup, or app-shell refactor is in scope here.
- Existing user-visible behavior should remain stable unless a confirmed bug requires correction.
- Derived surfaces must not become canonical owners of data as part of hardening.
- Configurable operational values must move toward config authority, not new source literals.
- Validation must stay real: no skipped gates, no weakened assertions, no fake-green shortcuts.
- Public route behavior must remain stable unless a contract fix is explicitly required and documented.
- During Claude’s frontend pass, `apps/mission-control/src/**` is frozen for this hardening lane unless explicitly carved out later.

## Goals

1. Make canonical ownership explicit so carsinOS stops accumulating overlapping truth surfaces.
2. Extract backend logic into clearer domain seams so gateway code becomes easier to reason about, test, and extend.
3. Normalize contracts and config ownership so runtime behavior is driven by durable config and typed protocol, not compatibility-era fallbacks.
4. Increase deterministic confidence in local validation, integration tests, and read-model behavior.
5. Improve runtime safety and observability for ship-readiness without changing product intent.

## Non-Goals

- redesigning Mission Control layouts
- changing tab names, tab order, or visual hierarchy
- visual polish, animation, spacing, typography, or CSS cleanup
- new product features beyond hardening-required bug fixes
- replacing current gateway/runtime architecture wholesale
- large migration of working code just for aesthetic cleanliness

## No-Touch Boundary

The following are out of scope unless a hard bug forces a surgical fix:

- page-level layout and styling in Mission Control
- onboarding copy, help copy, or guided-tour wording
- UI information architecture
- visual redesign of Connectors, Memory, Runbook, Strategy, Cockpit, Boards, Calendar, Focus, or Team
- `apps/mission-control/src/**` changes during Claude’s active audit window
- `apps/mission-control/e2e/**` changes during Claude’s active audit window, unless a backend regression makes a harness fix unavoidable and explicitly agreed

Permitted frontend work in this hardening pass is limited to:

- running existing frontend validation gates as regressions
- documenting frontend follow-up work as `POST_ANO`
- emergency backend-forced harness fixes only if execution is otherwise blocked and the collision is explicitly acknowledged

For this spec, “non-visual” means:

- no layout changes
- no copy or label changes
- no navigation changes
- no CSS or class-structure changes except narrowly scoped test hooks required for deterministic validation
- no component structure changes unless required to preserve existing behavior during contract alignment

## Hardening Freeze Rule

Claude is actively auditing and likely changing UI/UX surfaces outside this spec.

To avoid collision:

- non-UI hardening work may touch frontend/controller files only for contract alignment, server-driven bug correction, or deterministic test hooks
- any change to visual structure, copy, interaction wording, layout, styling, or navigation is `POST_ANO`
- if a shared controller or helper is touched for non-visual reasons, the change must preserve current UI behavior and stay narrowly scoped
- if a required non-visual change would materially collide with Claude’s audit output, that change pauses until the conflict is reconciled explicitly
- app-side contract/type/controller alignment is deferred until Claude’s audit and fix pass are complete unless explicitly re-opened

## Current System Risks This Pass Is Meant To Reduce

1. Canonical ownership drift:
   multiple surfaces can describe the same runtime state with slightly different semantics.
2. Gateway concentration debt:
   large gateway files own too much orchestration, read-model shaping, and policy glue.
3. Contract drift:
   protocol, gateway responses, and frontend types can diverge as new surfaces land quickly.
4. Compatibility-era fallback debt:
   some operational paths still depend on hardcoded defaults or permissive fallback behavior.
5. Validation fragility:
   broad success depends on mock state reset discipline, stable contracts, and non-ambiguous cross-surface assertions.
6. Read-model ambiguity:
   derived surfaces can accidentally imply they own state they only summarize.

## Canonical Ownership Model

Canonical ownership is defined at the domain level, not by whichever UI surface currently exposes the data most prominently.

### Canonical write owners

| Domain | Canonical Owner | Notes |
| --- | --- | --- |
| Goals / projects / tasks / task runtime links | Strategy domain | Strategy owns the durable management model. |
| Boards / columns / cards / board automations | Boards domain | Strategy may link, but does not own card lifecycle. |
| Jobs / schedules / history | Calendar/jobs domain | Calendar owns schedule truth; Runbook may derive from it. |
| Approvals / approval resolution state | Approvals domain | Focus and assistant surfaces consume this; they do not redefine it. |
| Sessions / runs / tool calls / delivery | Assistant runtime domain | Runbook and Cockpit summarize but do not own execution truth. |
| Connector sources / conversions / published tools / assignments / auth bindings / interactions | Connectors domain | Extensions mirrors connector-backed artifacts read-only. |
| Assistant memory bindings / assistant-to-memory routing | carsinOS memory binding domain | MNO remains canonical for assistant memory contents. |
| Memory contents / explainability / writeback proposals | MNO lane per assistant | carsinOS routes and exposes; it does not become memory truth. |
| Bootstrap presets / agent hierarchy metadata | Team / bootstrap domain | Runtime surfaces may display derived ownership context only. |

### Derived read-only domains

These surfaces must remain explicitly derived:

- Runbook
- Cockpit
- Focus queue summaries
- Extensions mirror entries for connector-backed tools
- cross-surface summary chips, badges, and counts

Derived read models must not:

- accept edits that bypass canonical owners
- invent alternate status semantics
- persist side effects outside their owning domain

If a derived surface currently exposes a mutation path, server-side enforcement must reject that path now unless the canonical owner explicitly owns the same action. UI cleanup of any exposed derived mutation affordance is `POST_ANO`.

Server-side enforcement audit is required for this pass:

- enumerate known mutation endpoints/actions reachable from derived surfaces in touched domains
- confirm they are either canonical-owner backed or explicitly rejected
- add or strengthen tests for those rejection paths where missing

### Enforcement definition

For this pass, “explicit and enforced” means all of the following:

- server-side mutation boundaries prevent derived surfaces from bypassing canonical owners
- read-model precedence rules are encoded in code, not only described in docs
- conflict behavior is covered by tests

### Conflict resolution rule

When canonical and derived state disagree, canonical state wins and the derived surface must either:

- surface the canonical value directly, or
- mark the derived value as degraded/stale/conflicted using a deterministic rule

Conflict handling must be deterministic and tested for at least these domain pairs:

- Strategy management state vs Runbook execution summary
- Calendar/job state vs Runbook execution summary
- Connectors registry state vs Extensions mirror state
- approvals domain state vs Focus queue summary state
- carsinOS assistant memory binding state vs derived memory status cards

## Hardening Workstreams

### Workstream 1: Canonical Truth Enforcement

Purpose:
make truth ownership durable and hard to misread in code.

Required outcomes:

- every major read model declares its canonical source domains
- derived status fields document precedence and conflict resolution
- helper paths that currently duplicate truth selection logic are centralized
- connector-backed tool mirrors remain explicitly read-only
- runbook, cockpit, focus, and summary read models stop carrying ambiguous ownership semantics

Rules:

- if two surfaces expose the same status, the canonical owner wins
- derived surfaces may summarize, badge, or link, but not reinterpret without an explicit precedence rule
- “latest”, “current”, “active”, and “ready” semantics must be deterministic and tested

### Workstream 2: Gateway Domain Extraction

Purpose:
reduce concentration inside gateway orchestration and read-model shaping.

Required outcomes:

- large cross-domain sections in gateway code move behind clearer domain modules/helpers
- route handlers become thinner and more consistent
- read-model assemblers are grouped by domain instead of interleaved ad hoc
- connector, strategy, runbook, memory, and approval logic stop leaking helper logic across unrelated zones

Priorities:

1. connector lifecycle and runtime tool exposure helpers
2. runbook read-model assembly and anchor resolution helpers
3. strategy summary / task-link shaping helpers
4. memory-binding and assistant memory status helpers
5. cross-surface capability/read-model normalization helpers

Rules:

- do not split code purely for aesthetics
- extraction must reduce coupling, not just create more files
- domain helpers must remain testable without end-to-end app boot when practical
- public route behavior, ordering, and response semantics must remain stable unless a contract fix is explicitly required by this spec
- extraction must preserve existing pagination, cursor, filtering, and ordering defaults unless a contract fix explicitly changes them

Success is measured by:

- thinner route handlers in the highest-risk integration zones
- extracted helpers with clear domain ownership
- targeted tests around extracted behavior
- fewer cross-domain helper dependencies in newly touched code

### Workstream 3: Contract and Schema Normalization

Purpose:
stop protocol, gateway, frontend, and mocks from drifting apart.

Required outcomes:

- protocol responses are the source of truth for exposed fields
- gateway handlers return complete typed payloads for all live surfaces
- frontend follow-up requirements are documented when protocol changes imply later Mission Control alignment
- mock or frontend parity changes are deferred `POST_ANO` during Claude’s active audit window
- newly added read-model fields are covered by contract tests or typed assertions

Focus areas:

- connector detail / conversion / tool describe responses
- strategy summary and task execution link responses
- runbook list/detail and deep-link target payload shape
- assistant memory status and explainability payloads
- cockpit summary widget data contracts

Rules:

- no silent optional-field drift when the field is required for current surfaces
- if a field is intentionally optional, the contract must say why and what fallback behavior is expected
- contract parity gaps between real gateway and mock gateway must be treated as bugs
- additive fields are allowed; removals or semantic field changes require a migration note and staged deployment/backout plan
- mixed-version behavior between gateway and Mission Control must fail soft where possible, not corrupt state or crash silently
- sorting, pagination, cursor, and ordering semantics must remain stable unless the contract explicitly changes them
- new fields must not change existing default behavior unless gated by config, version, or a documented bug fix that is explicitly accepted by this spec

Backward compatibility minimums:

- older Mission Control builds must ignore additive fields safely
- newer Mission Control builds must handle missing not-yet-deployed fields without crashing
- if a compatibility shim is needed, the shim and its removal condition must be documented in the implementation plan

Minimum parity requirement:

- at least one backend-level contract or serialization check is required per changed domain for:
  - Connectors
  - Memory
  - Runbook
  - Strategy
- any Mission Control mock-vs-gateway parity updates required by these changes are `POST_ANO` until the frontend lane is reopened

### Workstream 4: Config Authority and Hardcode Reduction

Purpose:
move operational behavior away from source literals and compatibility fallbacks.

Required outcomes:

- operational defaults touched by current ship surfaces are config-backed or fail-closed where appropriate
- remaining compatibility fallbacks are documented, explicit, and bounded
- config parsing, normalization, and precedence rules are centralized
- hardcoded-value guard remains green after the pass

Priority targets:

- connector runtime and policy defaults
- MNO compatibility-era fallback base URL / principal metadata paths
- channel/provider default behavior still sourced from literals where config should own it
- refresh cadence / polling / debounce values that belong in dedicated config modules

Persisted-config handling rules:

- if a newly authoritative config key is absent in persisted config, the spec must define whether behavior is:
  - migrated from legacy value
  - defaulted explicitly
  - fail-closed
- config normalization changes must preserve upgrade safety for existing local state
- if a compatibility fallback is removed from an operational path, the pass must define either:
  - a fallback-retention window, or
  - an explicit config readiness gate that prevents unsafe rollout

Initial in-scope fallback inventory for this pass:

- connector runtime and policy defaults touched by the Connectors surface
- MNO fallback base URL and principal metadata currently documented in the Numquam state map
- provider/channel defaults already identified in the hardcoded runtime audit
- read-model cadence values that currently live inline in operational paths

Rules:

- do not create new env-or-literal forks when a typed config contract should own the value
- defaults that affect external behavior must be visible and explainable
- dev-only fallback behavior must not silently leak into release-ready paths

### Workstream 5: Deterministic Validation and Mock Hygiene

Purpose:
make failures reproducible and reduce false confidence.

Required outcomes:

- backend/process/integration validation remains deterministic for all touched domains
- existing Mission Control e2e stays green as a regression gate, but frontend harness changes are deferred during Claude’s active audit window unless explicitly required
- tests assert real behavior rather than broad text coincidence
- backend tests cover ownership precedence and rollback behavior for derived surfaces

Rules:

- test fixes must resolve ambiguity or nondeterminism at the real source
- avoid brittle selector dependence where explicit stable hooks are warranted
- no “pass by timing luck” waits or retries as a substitute for real readiness conditions
- if deterministic test hooks are required in UI/controller files under Claude’s audit, those hooks must be non-visual, narrow, and treated as boundary-safe infrastructure only
- if such hooks would touch audited frontend files, the change pauses and is reclassified `POST_ANO` unless the user explicitly wants the overlap

Required test minimums:

- explicit precedence tests for “canonical owner wins” behavior
- explicit backend contract tests for changed response shapes
- regression coverage for ordering, cursor, or filtering behavior in touched read models
- migration or defaulting coverage for config-authority changes in touched paths
- pre/post invariant checks for touched read models where helper extraction could silently change “latest”, ordering, or filtered result semantics

### Workstream 6: Runtime Safety, Observability, and Performance

Purpose:
make the system easier to ship and operate without changing feature shape.

Required outcomes:

- read models expose freshness/staleness/degraded metadata consistently where needed
- runtime health surfaces do not silently swallow connector, memory, or read-model degradation
- expensive list/detail assembly paths are reviewed for obvious N+1 or repeated parsing waste
- summary/read-model computation uses shared helpers where repeated logic exists
- audit trails stay intact across connector, approval, runtime, and memory paths

Rules:

- prioritize correctness and observability before micro-optimization
- performance work must target known repeated work, not speculative churn
- new metrics or status fields must map to an operational decision, not vanity observability
- degraded connector or memory state must surface consistently through the relevant backend/read-model contract and be covered by tests in touched areas

## Rollout Strategy

This hardening pass should land as a behavior-preserving engineering stabilization pass.

Rollout rules:

- no new top-level user-facing modules are introduced here
- existing rollout flags remain authoritative
- any bug fix that changes visible behavior must be documented as a bug correction, not hidden as “hardening”
- backend extractions should preserve existing route shapes unless the contract fix is explicitly required
- removal of a compatibility fallback that can hard-fail production paths must be staged behind explicit config readiness or rollback-safe behavior

## Backout Strategy

Backout must stay practical.

Required backout properties:

- extracted helpers/modules can be reverted without schema rollback when possible
- protocol changes that widen responses should remain backward compatible during rollout
- any config normalization change must have a documented fallback/backout path
- read-model refactors must preserve canonical store data so backout is code-only, not data-recovery work
- if contract additions are consumed asymmetrically by gateway and Mission Control, backout must describe how each side behaves while temporarily out of sync

## Validation Requirements

This pass is not complete unless all applicable gates are green.

Minimum gates:

- Mission Control:
  - `npm run typecheck`
  - `npm run lint`
  - `npm run test:unit`
  - `npm run build`
  - `npm run test:e2e:full`
- Repo quality:
  - `python3 scripts/mission_control_quality_gate.py --profile pr`
  - `python3 scripts/mission_control_quality_gate.py --profile release --fail-on-blocked`
- Security / backend:
  - `scripts/security_pr_gate.sh`

Targeted proof expectations:

- ownership precedence tests for derived read models
- contract parity tests for mock vs gateway where current e2e depends on those payloads
- rollback/disable/re-enable behavior where connector, config, or summary state can drift
- deterministic process/integration coverage for new boundary helpers
- regression checks for preserved ordering/pagination/filter defaults in touched read models

Stop-ship conditions:

- canonical-vs-derived conflict tests fail
- mock-vs-gateway parity tests fail for a changed domain
- a required frontend-file change would overlap Claude’s active audited surface without explicit approval
- extraction changes ordering, cursor, or filtering semantics without an explicit accepted contract change
- a derived mutation path remains writable without canonical-owner authorization or explicit rejection

## Implementation Constraints

- no destructive repo cleanup outside this scoped work
- no reverting unrelated user changes
- no UI polish bundled into this pass
- no hidden behavior changes justified as “cleanup”
- no reward hacking: gates must pass because the system is correct, not because coverage was weakened

## Initial Touchpoint Map

This is intentionally high-level; SpecSwarm should refine it.

### Protocol

- `crates/carsinos-protocol/src/lib.rs`

### Storage

- `crates/carsinos-storage/src/lib.rs`
- `migrations/0004_agent_memory_bindings.sql`
- `migrations/0005_connector_registry.sql`

### Gateway

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-gateway/src/runbook.rs`
- `crates/carsinos-gateway/tests/e2e_process.rs`

### Supporting audits/specs

- `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`
- `docs/CLIDEX_HARDCODE_REMEDIATION_TASK.md`
- `docs/CARSINOS_UNIFIED_CONNECTOR_REGISTRY_SPEC.md`
- `docs/CARSINOS_VISUAL_RUNBOOK_SPEC.md`
- `docs/MNO_CARSINOS_ASSISTANT_MEMORY_INTEGRATION_SPEC.md`

### Workstream-specific implementation map

#### Workstream 1: Canonical truth enforcement

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-storage/src/lib.rs`
- tests:
  - `crates/carsinos-gateway/tests/e2e_process.rs`
  - focused storage/gateway precedence tests for touched domains

#### Workstream 2: Gateway domain extraction

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-gateway/src/runbook.rs`
- tests:
  - extracted helper unit coverage where practical
  - `crates/carsinos-gateway/tests/e2e_process.rs`

#### Workstream 3: Contract/schema normalization

- `crates/carsinos-protocol/src/lib.rs`
- tests:
  - backend serialization / route contract assertions
  - domain contract assertions for changed payloads

#### Workstream 4: Config authority / hardcode reduction

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-storage/src/lib.rs`
- `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`
- tests:
  - `scripts/security_hardcoded_value_guard.py`

#### Workstream 5: Deterministic validation / mock hygiene

- backend process/integration tests under gateway/storage/protocol
- existing Mission Control e2e gate runs as regression only

#### Workstream 6: Runtime safety / observability

- `crates/carsinos-gateway/src/main.rs`
- tests:
  - backend health/read-model coverage for degraded metadata
  - contract assertions for surfaced degradation state

### Deferred `POST_ANO` touchpoints after Claude’s frontend lane settles

- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/types.ts`
- `apps/mission-control/src/lib/opsUxConfig.ts`
- `apps/mission-control/src/app/useAppController.ts`
- `apps/mission-control/src/app/useRuntimeConnectionController.ts`
- `apps/mission-control/src/features/strategy/`
- `apps/mission-control/src/features/runbook/`
- `apps/mission-control/src/features/memory/`
- `apps/mission-control/src/features/connectors/`
- `apps/mission-control/e2e/mockGateway.mjs`
- `apps/mission-control/e2e/testHarness.ts`
- `apps/mission-control/e2e/*.spec.ts`
- `apps/mission-control/src/lib/api.test.ts`
- `apps/mission-control/src/lib/opsUxConfig.test.ts`

## Exit Criteria

This hardening pass is complete when:

1. canonical ownership is explicit and enforced across current integrated domains
2. gateway concentration is reduced in the highest-risk integration zones
3. protocol/gateway/frontend/mock contract drift is materially reduced
4. remaining operational hardcode debt in touched areas is either remediated or explicitly documented and bounded
5. deterministic validation is green across Mission Control, repo quality gates, and security gate
6. no intentional UI/UX redesign work has leaked into the pass
