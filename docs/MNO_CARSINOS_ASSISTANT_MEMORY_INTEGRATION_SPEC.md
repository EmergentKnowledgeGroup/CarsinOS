# MNO carsinOS Lane-Scoped Memory Integration Spec

Date: 2026-03-25
Status: Active target architecture
Scope: `carsinOS` integration with standalone `ModelNumquamOblita`

## Historical Note

An earlier version of this doc described `assistant-bound` MNO topology.

That older model was acceptable as a narrow first slice, but it is not the correct long-term architecture for multi-user assistant sharing. This updated spec supersedes the assistant-bound interpretation and replaces it with a **lane-scoped** model keyed by:

- `human_identity + assistant_agent`

## 1. Purpose

Integrate standalone MNO into `carsinOS` as the lane-scoped memory backend for one or more user-assistant lanes without creating a shared multi-assistant memory core.

This spec defines the `carsinOS`-side implementation contract and product behavior for:

- lane-scoped MNO orchestration
- lane-scoped Memory tab/operator UX in Mission Control
- use of MNO `integration-v1` for orchestration
- use of selected native MNO runtime APIs for Memory tab reads and bounded operator actions

This spec assumes the following ownership split:

- `carsinOS` owns routing, persona, provider execution, and Mission Control
- `MNO` owns memory truth, retrieval, explainability, and writeback semantics

## 2. Goals

1. Support one MNO runtime instance per active `human_identity + assistant_agent` lane.
2. Keep user-assistant memory isolated by default and by architecture.
3. Use stable `integration-v1` for runtime orchestration and writeback lifecycle.
4. Add a first-class `Memory` tab in Mission Control without breaking current runtime UX flow.
5. Reuse MNO-native read/operator contracts for cards, episodes, graph drilldown, explainability, and health instead of re-implementing memory semantics in `carsinOS`.
6. Keep final prompt composition and provider execution in `carsinOS`.

## 3. Non-Goals

- No shared multi-assistant MNO memory core in this phase.
- No shared cross-user MNO lane by default.
- No client-side graph neighborhood synthesis in `carsinOS`.
- No dependence on MCP as the `carsinOS` production hot path.
- No requirement to embed or mirror standalone MNO’s browser UI.
- No assumption that native MNO mutation HTTP endpoints are a stable public external contract.

## 4. Locked Decisions

### 4.1 Isolation model

`carsinOS` will treat MNO as:

- one reusable MNO codebase
- one lane-scoped MNO runtime process/sidecar per active user-assistant lane

There is no shared assistant memory core in this architecture.

In v1:

- each active lane has zero or one active MNO binding
- each active binding points to exactly one isolated MNO runtime lane
- each active MNO runtime lane serves exactly one `human_identity + assistant_agent` pair

This is a hard invariant, not a preference.

Each active lane gets its own:

- MNO base URL / sidecar binding
- runtime state root
- memory store lane
- reviewed episode lane
- diagnostics / audit lane on the MNO side

### 4.2 Routing ownership

`carsinOS` owns:

- human identity resolution
- assistant selection
- routing runs to the correct lane-scoped MNO instance
- final provider selection and final provider call
- final prompt composition
- operator UX around lane selection and memory visibility

MNO owns:

- memory truth
- retrieval and explainability semantics
- mutation proposal semantics
- graph traversal semantics and truncation behavior

### 4.3 Contract split

`carsinOS` will use:

- `integration-v1` for orchestration and lifecycle
- selected native MNO runtime APIs for Memory tab/operator reads
- the native graph-neighborhood API for graph drilldown

MCP remains first-class for parity, tooling, and diagnostics, but `carsinOS` does not treat MCP as the primary production integration target.

Mission Control will consume native MNO operator data through gateway-mediated wrappers/read models in v1.

Direct frontend-to-MNO calls are out of scope for this phase.

### 4.4 Prompt composition contract

`MNO` provides:

- bounded memory context
- evidence packages
- explainability data

`carsinOS` preserves:

- assistant identity and persona
- system prompt
- runtime policy
- provider-specific formatting

`carsinOS` must not pass a fully MNO-authored message list straight through untouched and must not hand final provider ownership to MNO.

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

## 5. Lane Provisioning Model

### 5.1 Lane key

Recommended default:

- `lane_key = human_identity + assistant_agent`

That gives:

- one user-assistant memory lane by default
- no accidental cross-user memory bleed
- clean future policy binding

### 5.2 Runtime root

Each lane gets its own runtime state root.

Use:

- `MNO_RUNTIME_STATE_ROOT=<lane-specific-path>`

This matters because MNO runtime state still contains lane-local artifacts like:

- wizard state
- diagnostics
- quicknote state
- methodology state
- reports
- live runtime locks

### 5.3 Store and reviewed episodes

Each active lane should bind:

- one memories store
- optionally one reviewed episode artifact

Launch shape:

```bash
MNO_RUNTIME_STATE_ROOT=/path/to/carsinos/lanes/<lane_id>/runtime \
python3 tools/run_live_runtime.py \
  --host 127.0.0.1 \
  --port <lane_port> \
  --memories /path/to/<lane>/atoms.sqlite3 \
  --episodes /path/to/<lane>/episode_cards.reviewed.json
```

### 5.4 Lane readiness

Before using the lane, `carsinOS` should call:

- `GET /api/integration/v1/health`
- `GET /api/integration/v1/capabilities`

Use that as the lane readiness gate inside `carsinOS`.

## 6. Runtime Flow

The intended hot loop is:

1. resolve `human_identity`
2. resolve `assistant_agent`
3. resolve the selected memory policy
4. resolve the active MNO lane, if any
5. call `context/build` before provider completion
6. compose the final prompt in `carsinOS`
7. call the provider
8. call `writeback/propose`
9. operator or policy decides
10. call `writeback/resolve`

If the selected lane’s MNO runtime is degraded or unavailable:

- `carsinOS` may fall back only to stateless or lane-local non-MNO behavior that does not touch another lane’s memory
- degradation must be surfaced in operator-visible status
- routing must not silently spill into another lane’s MNO runtime

## 7. Native MNO APIs carsinOS Will Consume

### 7.1 Orchestration contract

Use `integration-v1` for:

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
- lane correlation
- principal metadata where available

### 7.2 Read/operator surfaces

Mission Control Memory tab should build around these native MNO reads:

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

Trusted local operator actions may additionally expose:

- `GET /api/memory/proposals`
- `POST /api/memory/proposals/{proposal_id}/approve`
- `POST /api/memory/proposals/{proposal_id}/reject`
- `POST /api/memory/episodes/{episode_id}/disable`
- `POST /api/memory/episodes/{episode_id}/enable`
- `POST /api/memory/episodes/{episode_id}/edit`

These are treated as trusted internal/operator surfaces, not as frozen public contracts.

## 8. Mission Control UX

### 8.1 Memory tab scope

The Memory tab should be lane-scoped, not assistant-global by default.

An operator must be able to tell:

- which human identity is active
- which assistant is active
- which lane key is active
- whether the lane is local-only, MNO-backed, or disabled

### 8.2 Day-one surfaces

Good day-one Mission Control surfaces are:

- lane health
- capabilities
- current lane binding
- cards
- episodes
- graph / graph neighborhood
- why / explainability
- proposal queue
- runtime telemetry summary

### 8.3 Adjacent surfaces

Additive memory-aware context may also appear in:

- `Cockpit` for lane health/degrade visibility
- assistant/run detail surfaces for `context.why`
- proposal/approval surfaces if trusted operator actions are enabled

## 9. Data and State Rules

1. No cross-user memory reads by default.
2. No cross-user writeback or proposal resolution by default.
3. No global “default MNO” fallback that can accidentally mix lanes.
4. Every Memory tab request must be bound to an explicit lane.
5. All operator-visible status must clearly identify which lane is being shown.
6. If a lane has no configured MNO binding, the UI must show a clean empty/unconfigured state rather than a degraded shared fallback.
7. Cache keys and in-flight requests must be partitioned by lane identity.
8. If the active human, assistant, or lane changes, stale responses for the previous lane must be discarded.
9. All why/citation/graph/card/episode/proposal requests must be scoped server-side to the current lane.
10. Any detected cross-lane data exposure is a stop-ship failure.

## 10. Auth and Security

1. `integration-v1` bearer auth and role rules remain authoritative.
2. `carsinOS` must not grant rights through envelope metadata alone.
3. Mutation actions require trusted local operator mode and the correct MNO-side role.
4. MNO-side auth failures must surface explicitly in Mission Control.
5. `carsinOS` must not store raw long-lived secrets in frontend state.
6. If gateway wrappers are used, the gateway is responsible for secret handling and header injection.
7. Memory read surfaces require explicit read authorization/capability checks.
8. Gateway must reject untrusted or malformed MNO target configuration and must not proxy arbitrary URLs.
9. Operator write actions must produce durable audit evidence through the MNO-side audit system and `carsinOS` correlation records.

## 11. Rollout and Backout

### Rollout

1. ship lane-scoped backend bindings and read models
2. ship read-only Memory tab first
3. gate trusted operator actions in a follow-on release after read-only flows are green

### Backout

If the native read/operator integration causes problems:

- disable the affected lane or Memory tab sections behind per-lane and global feature controls
- keep `integration-v1` orchestration intact
- preserve current chat/runtime flows

Stop-ship / immediate rollback conditions:

- any cross-lane data exposure
- any observed shared/default MNO fallback across lanes
- any unauthorized Memory-tab read exposure

## 12. Testing and Validation

### Backend

- per-lane binding selection
- correct `integration-v1` routing by lane
- no cross-lane fallback
- native MNO read wrapper coverage
- graph-neighborhood parameter and truncation passthrough
- auth failure propagation
- capability probe / missing-endpoint downgrade behavior
- lane change invalidation behavior
- base URL validation / proxy safety behavior

### Frontend

- Memory tab empty/unconfigured state
- lane-scoped state changes
- graph overview + neighborhood drilldown behavior
- detail/explainability rendering
- degrade/error states
- cache partitioning by lane
- stale-response discard on lane change
- not-authorized UX for read surfaces

### End-to-end

- the same assistant assigned to two different humans resolves to two different MNO lanes and never leaks data across lanes
- `context.build` / `writeback.propose` route to the selected lane MNO
- Memory tab loads cards, episodes, graph overview, and graph neighbors for the selected lane
- graph truncation/limit state is surfaced honestly
- why/citation flow works from the selected lane
- orchestration/native health mismatch is surfaced honestly
- proposal actions only work in trusted local operator mode
- unsupported native endpoints degrade non-fatally

## 13. Authoritative External References

This spec should stay aligned with:

- `/Volumes/ultariumv3/modelNumquamOblita/docs/CARSINOS_INTEGRATION_REQUIREMENTS.md`
- `/Volumes/ultariumv3/modelNumquamOblita/docs/CARSINOS_APPDEX_RESPONSE_FROM_MNOCODEX_2026-03-25.md`

If this file and those external MNO-side contract docs disagree about integration boundaries, transport ownership, or runtime topology, the conflict must be resolved before implementation.
