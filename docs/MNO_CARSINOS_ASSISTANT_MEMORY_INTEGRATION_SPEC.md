# MNO carsinOS Assistant Memory Integration Spec

Date: 2026-03-12
Status: Implemented locally and green on 2026-03-12
Scope: `carsinOS` integration with standalone `ModelNumquamOblita`

## Implementation Status

This spec is now implemented locally for Phase 0 and Phase 1.

Validated locally on 2026-03-12 with:

- Mission Control frontend `typecheck`, `lint`, `test:unit`, `build`
- targeted Memory e2e
- full Mission Control Playwright suite
- Mission Control quality gates (`pr`, `release --fail-on-blocked`)
- local `security_pr_gate.sh`

## 1. Purpose

Integrate standalone MNO into carsinOS as the assistant-bound memory backend for one or more assistants without creating a shared cross-assistant memory core.

This spec defines the carsinOS-side implementation contract and product behavior for:

- assistant-scoped MNO orchestration
- assistant-scoped Memory tab/operator UX in Mission Control
- use of MNO `integration-v1` for orchestration
- use of selected native MNO runtime APIs for Memory tab reads and bounded operator actions

This spec does not wait on standalone MNO UI polish. It builds against the contracts that are already frozen or explicitly blessed in the handoff packet.

## 2. Goals

1. Support one MNO runtime instance per assistant/profile/store.
2. Keep assistant memory isolated by default and by architecture.
3. Use stable `integration-v1` for runtime orchestration and writeback lifecycle.
4. Add a first-class `Memory` tab in Mission Control without breaking current runtime UX flow.
5. Reuse MNO-native read/operator contracts for cards, episodes, graph drilldown, explainability, and health instead of re-implementing memory semantics in carsinOS.
6. Keep graph traversal semantics authoritative on the MNO side.

## 3. Non-Goals

- No shared multi-assistant MNO memory core in this phase.
- No client-side graph neighborhood synthesis in carsinOS.
- No dependence on MCP as the carsinOS product target.
- No requirement to embed or mirror standalone MNO’s browser UI.
- No attempt to redesign MNO’s internal tenant model beyond consuming one assistant-bound sidecar at a time.
- No assumption that native MNO mutation HTTP endpoints are a stable public external contract.

## 4. Locked Decisions

### 4.1 Isolation model

carsinOS will treat MNO as:

- one reusable MNO codebase
- one assistant-bound MNO runtime process/sidecar per assistant/profile/store

There is no shared assistant memory core in this architecture.

In v1:

- each assistant has zero or one active MNO binding
- each active binding points to exactly one isolated MNO runtime lane
- each MNO runtime lane serves exactly one assistant binding

This is a hard invariant, not a preference.

Each assistant gets its own:

- MNO base URL / sidecar binding
- auth context
- memory store lane
- reviewed episode lane
- diagnostics / audit lane on the MNO side

### 4.2 Routing ownership

carsinOS owns:

- assistant selection
- routing runs to the correct assistant-bound MNO instance
- operator UX around assistant selection and memory visibility

MNO owns:

- memory truth
- retrieval and explainability semantics
- mutation proposal semantics
- graph traversal semantics and truncation behavior

### 4.3 Contract split

carsinOS will use:

- `integration-v1` for orchestration and lifecycle
- selected native MNO runtime APIs for Memory tab/operator reads
- the native graph-neighborhood API for graph drilldown

MCP may remain for tooling parity, but carsinOS does not treat MCP as the primary product integration target.

Mission Control will consume native MNO operator data through gateway-mediated wrappers/read models in v1.

Direct frontend-to-MNO calls are out of scope for this phase.

### 4.4 Graph ownership

carsinOS must not:

- invent custom graph traversal ordering
- synthesize graph neighborhoods client-side
- hide server truncation/limit state

carsinOS must treat MNO’s graph responses as authoritative, including:

- BFS ordering
- returned nodes/links
- `depth`
- `node_limit`
- `link_limit`
- `requests_used`
- truncation truth

### 4.5 Health precedence

`integration-v1` health is authoritative for orchestration readiness.

Native `GET /api/runtime/health` is supplemental diagnostic data for operators.

If they disagree:

- orchestration health governs top-level ready/degraded/unavailable state
- native runtime health is shown as secondary diagnostic detail
- Mission Control must surface a mismatch warning instead of silently preferring the healthier result

### 4.6 Read access control

Memory read surfaces require explicit read authorization/capability checks.

Mutation gating alone is not sufficient.

## 5. Current State Summary

### 5.1 On the MNO side

Ready now:

- `integration-v1` HTTP contract
- MCP parity for `integration-v1`
- native memory/operator HTTP APIs
- native graph-neighborhood API
- per-runtime single-lane memory behavior

Known MNO constraints:

- current runtime roots are global inside one repo lane
- latest wizard pointer is global
- proposal queue is process-local and in-memory
- native operator mutation APIs are useful but not a frozen public contract

### 5.2 On the carsinOS side

carsinOS already has a Numquam/MNO integration spine in protocol and gateway, but Mission Control does not yet expose a real assistant-scoped Memory tab or first-class native MNO operator surfaces.

## 6. Product Scope

This build is split into three phases.

### Phase 0: Assistant-Bound MNO Topology

Deliver:

- assistant -> MNO binding model in carsinOS
- per-assistant MNO status visibility
- runtime orchestration routed to the selected assistant’s MNO instance
- additive config and read models required to expose this cleanly

### Phase 1: Memory Tab Read Surface

Deliver:

- a new additive `Memory` tab in Mission Control
- assistant selector or assistant-bound context visibility
- read-only and inspect-first operator workflows:
  - health
  - cards
  - atom detail
  - episodes
  - graph overview
  - graph neighborhood drilldown
  - turn why / citations
  - telemetry / decision reasons

### Phase 2: Trusted Operator Actions

Deliver as gated and additive:

- proposal inbox visibility
- approve/reject proposal actions
- episode enable/disable/edit actions when bound to a trusted local MNO runtime

Phase 2 is a follow-on release after Phase 0 and Phase 1 are green.

## 7. Backend Contract Requirements In carsinOS

### 7.1 Assistant-bound MNO binding model

carsinOS must introduce a canonical assistant-memory binding model that can answer:

- which assistant uses MNO
- which base URL belongs to that assistant
- which auth profile/token source belongs to that assistant
- whether the binding is enabled, degraded, or unavailable
- which native read surfaces are exposed for trusted local operator use

Minimum binding fields:

- `assistant_id`
- `provider_kind` or backend kind
- `mno_base_url`
- `auth_mode`
- `auth_reference` or token source handle
- `enabled`
- `trusted_local_operator_actions`
- `binding_id`
- `binding_status`
- `native_surface_availability`

If the existing agent/assistant config model already contains equivalent fields, reuse and extend it rather than creating a parallel configuration island.

Binding uniqueness rules in v1:

- one assistant may have at most one active MNO binding
- one binding belongs to exactly one assistant
- switching a binding is an explicit reconfiguration event, not an implicit runtime fallback

`binding_status` must resolve to:

- `unconfigured`
- `available`
- `degraded`
- `unavailable`
- `unauthorized`

State rules:

- `unconfigured`: no binding exists for the assistant
- `available`: `integration-v1` orchestration health is good and required native read surfaces for the current UI are reachable
- `degraded`: orchestration health is reachable but warnings, degraded capability, or missing non-critical native surfaces exist
- `unavailable`: binding exists but orchestration health cannot be used safely
- `unauthorized`: binding exists but auth is missing, expired, invalid, or insufficient for the requested surface

Assistant binding changes invalidate active Memory-tab requests and caches immediately.

### 7.2 `integration-v1` orchestration

carsinOS gateway must continue to own runtime orchestration and call the selected assistant-bound MNO runtime for:

- `context.build`
- `context.why`
- `writeback.propose`
- `writeback.resolve`
- `health.get`
- `capabilities.get`

All orchestration requests must preserve:

- `request_id`
- `session_id`
- `run_id`
- assistant correlation
- principal metadata where available

### 7.3 Degrade and fallback behavior

If the selected assistant’s MNO runtime is degraded or unavailable:

- carsinOS may fall back only to stateless or assistant-local non-MNO behavior that does not touch another assistant’s memory lane
- degradation must be surfaced in operator-visible status
- assistant routing must not silently spill into another assistant’s MNO runtime

No cross-assistant fallback is allowed.

If auth is missing or expired:

- carsinOS must treat the lane as `unauthorized`
- native read/operator surfaces for that assistant must move into a locked/not-authorized state
- orchestration retries must be bounded and visible to operators

### 7.4 Native capability and version gating

carsinOS must not assume every native MNO surface is present just because `integration-v1` is present.

Gateway binding initialization must:

1. validate `integration-v1` contract availability
2. probe required native read surfaces
3. publish per-surface availability flags to Mission Control

If a native surface is absent, unsupported, or non-conforming:

- that surface must be hidden or downgraded non-fatally
- the rest of the assistant lane may remain usable if orchestration is still healthy

At minimum, gateway must gate:

- `graph_map`
- `graph_neighbors`
- `episodes`
- `turn_why`
- `citations`
- `telemetry`
- `proposals`

### 7.5 Native read/operator proxy layer

carsinOS must add a clean integration layer for selected native MNO runtime APIs used by Mission Control.

carsinOS-native wrappers/read models must preserve the underlying MNO semantics rather than inventing alternate query behavior.

Gateway must validate `mno_base_url` against trusted configuration and must not allow arbitrary user-supplied MNO targets at request time.

## 8. Native MNO APIs carsinOS Will Consume

### 8.1 Read surfaces

Mission Control Memory tab will build around these native MNO reads:

- `GET /api/memory/cards`
- `GET /api/memory/cards/{card_id}`
- `GET /api/memory/atom/{atom_id}`
- `GET /api/memory/graph-map`
- `GET /api/memory/graph/neighbors`
- `GET /api/memory/episodes`
- `GET /api/turns/{turn_id}/why`
- `GET /api/archive/citation/{citation_token}`
- `GET /api/runtime/health`
- `GET /api/runtime/telemetry/summary`
- `GET /api/runtime/telemetry/turns`
- `GET /api/runtime/decision-reasons`

Endpoint role rules:

- `graph-map` is the authoritative graph overview surface
- `graph/neighbors` is the authoritative bounded drilldown surface
- `GET /api/memory/graph` is not used for the primary Memory graph canvas in v1 and may be used only for narrow atom-detail diagnostics if needed
- `integration-v1 context.why` is used for orchestration/run-lifecycle explainability
- `GET /api/turns/{turn_id}/why` is used for Memory-tab historical turn detail and operator explainability panels

### 8.2 Trusted local operator actions

When `trusted_local_operator_actions=true`, carsinOS may expose:

- `GET /api/memory/proposals`
- `POST /api/memory/proposals/{proposal_id}/approve`
- `POST /api/memory/proposals/{proposal_id}/reject`
- `POST /api/memory/episodes/{episode_id}/disable`
- `POST /api/memory/episodes/{episode_id}/enable`
- `POST /api/memory/episodes/{episode_id}/edit`

These are treated as trusted internal/operator surfaces, not as frozen public contracts.

`trusted_local_operator_actions` means all of the following are true:

- the binding is explicitly flagged for trusted local operator mode in backend config
- gateway is mediating the request
- MNO-side auth confirms the required role
- the requesting Mission Control operator is authorized to use the surface

UI-only trust toggles are forbidden.

### 8.3 Pagination, limits, and payload budgets

carsinOS wrappers/read models must expose bounded query semantics even when translating to MNO-native query shapes.

Minimum wrapper rules:

- cards: required `limit`, bounded search/filter, stable cursor or offset semantics
- episodes: required `limit`, bounded search/filter, stable cursor or offset semantics
- telemetry turns: required `limit`, bounded recency window, stable cursor or offset semantics
- graph-map: explicit bounded overview payload
- graph-neighbors: explicit `depth`, `node_limit`, `link_limit`

Maximum defaults in v1:

- cards/episodes page size default `50`, max `100`
- telemetry-turns page size default `50`, max `100`
- graph-neighbors defaults and maximums must defer to the MNO-side contract and be surfaced honestly in response payloads

Large payloads must fail closed or truncate truthfully rather than silently overflowing the client.

## 9. Mission Control UX

### 9.1 Additive navigation rule

The Memory experience must land without interrupting the current runtime-first UX.

That means:

- additive tab/surface only
- current assistant, strategy, runbook, boards, calendar, focus, cockpit flow remains intact
- no shortcut churn unless the final tab map demands it and the change is intentional

### 9.2 Memory tab content model

The Memory tab should be assistant-scoped and composed of:

- assistant identity strip
- MNO health / degrade / telemetry summary
- graph overview pane
- graph neighborhood drilldown
- cards/episodes search and list pane
- detail panel for card / atom / episode / turn why data
- citation inspector

The active assistant binding must be visible enough that an operator can tell at a glance which assistant memory lane is being viewed.

Preferred IA:

- top summary strip
- left browse/filter column
- center graph/list workspace
- right detail/explainability panel

### 9.3 Graph split

Use:

- `/api/memory/graph-map` for overview canvas
- `/api/memory/graph/neighbors` for local expansion/drilldown
- card/atom/why/citation endpoints for detail

### 9.4 Explainability

The Memory tab must make it easy to answer:

- why this memory was retrieved
- what evidence backs it
- what citations exist
- which assistant memory lane it came from

### 9.5 Adjacent surfaces

Additive Memory-aware context may also appear in:

- `Cockpit` for assistant-bound MNO health/degrade visibility
- assistant/run detail surfaces for `context.why`
- proposal/approval surfaces if Phase 2 lands

Per-assistant kill switches must be respected across:

- Memory tab
- Cockpit memory-derived widgets
- assistant detail memory-derived panels
- any proposal or episode action surface

If a lane is disabled, those surfaces must hide or clearly lock rather than showing stale memory state.

## 10. Data and State Rules

1. No cross-assistant memory reads by default.
2. No cross-assistant writeback or proposal resolution.
3. No global “default MNO” fallback that can accidentally mix assistant identity cores.
4. Every Memory tab request must be bound to an explicit assistant memory lane.
5. All operator-visible status must clearly identify which assistant lane is being shown.
6. If an assistant has no configured MNO binding, the UI must show a clean empty/unconfigured state rather than a degraded shared fallback.
7. Cache keys and in-flight requests must be partitioned by `binding_id`.
8. If the active assistant or binding changes, stale responses for the previous binding must be discarded and must not update the current UI.
9. All why/citation/graph/card/episode/proposal requests must be scoped server-side to the current assistant lane.
10. Any detected cross-assistant data exposure is a stop-ship failure.

## 11. Auth and Security

1. `integration-v1` bearer auth and role rules remain authoritative.
2. carsinOS must not grant rights through envelope metadata alone.
3. Mutation actions require trusted local operator mode and the correct MNO-side role.
4. MNO-side auth failures must surface explicitly in Mission Control.
5. carsinOS must not store raw long-lived secrets in frontend state.
6. If gateway wrappers are used, the gateway is responsible for secret handling and header injection.
7. Memory read surfaces require explicit read authorization/capability checks.
8. Gateway must reject untrusted or malformed MNO target configuration and must not proxy arbitrary URLs.
9. Operator write actions must produce durable audit evidence through the MNO-side audit system and carsinOS correlation records.

## 12. Observability

carsinOS must surface enough per-assistant MNO status to debug:

- health state
- capabilities and contract availability
- latency/degrade warnings where available
- telemetry summary
- recent turn-level telemetry relevant to Memory tab workflows

Logs and read models must preserve assistant correlation so operators can distinguish Claude’s memory lane from Lyra’s or any other assistant lane.

When orchestration health and native runtime health disagree, Mission Control must expose:

- authoritative orchestration state
- native runtime diagnostic state
- explicit mismatch warning

## 13. Rollout and Backout

### Rollout

1. ship assistant-bound backend bindings and read models
2. ship read-only Memory tab first
3. gate trusted operator actions in a follow-on release after read-only flows are green

### Backout

If the native read/operator integration causes problems:

- disable the affected assistant lane or Memory tab sections behind per-assistant and global feature controls
- keep `integration-v1` orchestration intact
- preserve current chat/runtime flows

Backout must not require removing the assistant-bound orchestration wiring.

Stop-ship / immediate rollback conditions:

- any cross-assistant data exposure
- any observed shared/default MNO fallback across assistants
- any unauthorized Memory-tab read exposure

Backout must define behavior for:

- binding-model fields remaining in config after UI disable
- Cockpit or assistant-detail memory-derived surfaces hiding in sync with the disabled lane
- per-assistant disable without taking down every Memory surface globally

## 14. Implementation Touchpoints Map

Likely carsinOS touchpoints for this spec:

- `crates/carsinos-protocol/src/lib.rs`
  - assistant-memory binding model
  - native MNO wrapper DTOs
  - availability/read-role/status types
- `crates/carsinos-gateway/src/main.rs`
  - assistant-bound binding resolution
  - `NumquamClient` selection by assistant binding
  - native MNO read/operator wrappers
  - capability/health probes and per-surface availability flags
- `apps/mission-control/src/app/tabs.ts`
  - additive `Memory` tab registration
- `apps/mission-control/src/app/useAppController.ts`
  - Memory tab activation and open-item state
- `apps/mission-control/src/app/AppContent.tsx`
  - Memory page/controller wiring and cross-surface open flows
- `apps/mission-control/src/lib/api.ts`
  - Memory-tab API wrappers
- `apps/mission-control/src/types.ts`
  - Memory-tab DTOs and availability models
- `apps/mission-control/src/features/memory/*`
  - new Memory tab controller and UI
- `apps/mission-control/src/features/assistant/*`
  - assistant-bound explainability/context links
- `apps/mission-control/src/features/cockpit/*`
  - additive assistant-bound MNO health widgets
- `apps/mission-control/e2e/*`
  - assistant-bound Memory flows and no-leak regression coverage

Current collision risk:

- Mission Control shell/tab/app wiring files are already dirty in the worktree
- gateway and protocol files are already dirty in the worktree

Implementation must work with existing changes and must not revert unrelated work.

## 15. Testing and Validation

### Backend

- per-assistant binding selection
- correct `integration-v1` routing by assistant
- no cross-assistant fallback
- native MNO read wrapper coverage
- graph-neighborhood parameter and truncation passthrough
- auth failure propagation
- capability probe / missing-endpoint downgrade behavior
- binding change invalidation behavior
- base URL validation / proxy safety behavior

### Frontend

- Memory tab empty/unconfigured state
- assistant-scoped state changes
- graph overview + neighborhood drilldown behavior
- detail/explainability rendering
- degrade/error states
- no regression to current tab flow
- cache partitioning by assistant binding
- stale-response discard on binding change
- not-authorized UX for read surfaces

### End-to-end

- Assistant A and Assistant B point to different MNO lanes and never leak data across lanes
- `context.build` / `writeback.propose` route to the selected assistant MNO
- Memory tab loads cards, episodes, graph overview, and graph neighbors for the selected assistant
- graph truncation/limit state is surfaced honestly
- why/citation flow works from the selected assistant lane
- orchestration/native health mismatch is surfaced honestly
- proposal actions only work in trusted local operator mode
- unsupported native endpoints degrade non-fatally

### Required local gates before PR

- `npm run typecheck`
- `npm run lint`
- `npm run test:unit`
- `npm run build`
- targeted e2e for Memory tab and assistant-bound routing
- targeted Rust tests for gateway/protocol changes

## 16. Resolved SpecSwarm Decisions

1. Phase 2 trusted operator actions are not part of the first implementation batch; Phase 0 and Phase 1 ship first.
2. Gateway-mediated native MNO wrappers are the only allowed frontend path in v1.
3. `integration-v1` health is authoritative for orchestration status; native runtime health is supplemental.
4. `graph-map` and `graph/neighbors` are the only authoritative graph canvases in the Memory tab.
