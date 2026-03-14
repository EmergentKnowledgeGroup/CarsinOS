# carsinOS Visual Runbook Spec

Generated: 2026-03-09

## Executive Summary

carsinOS should ship a first-class visual runbook layer as a real Mission Control product surface.

Version 1 must ship as a plain, readable, live runbook view, not an animated office or decorative flowchart.

The runbook backbone must be real product logic: deterministic, queryable, grounded in persisted workflow facts, and reusable by future visual layers.

Version 1 should not introduce a second execution engine or a user-authored workflow builder. It should normalize existing carsinOS workflow truth into a canonical runbook model.

The future office view is explicitly a later presentation layer on top of the same backbone.

## Product Decision

carsinOS will add a new Mission Control `Runbook` product layer that exposes the real execution map for supported workflow kinds.

The runbook is not:

- a separate hidden orchestration engine
- a fake animation surface
- a cockpit-only widget
- a narrative explanation generated after the fact

The runbook is:

- a canonical execution read model
- a shared language for operator, assistant, orchestrator, and future visual layers
- a product-center surface that makes state, blockers, approvals, handoffs, and next actions visible

## Goals

Version 1 must make these things visible and trustworthy:

- what work is supposed to happen
- which step is active now
- what already happened
- what the system is waiting on
- what is blocked
- what failed
- what the next valid step is
- which artifacts and records are involved

Version 1 must improve:

- operator clarity
- operator trust
- triage speed
- workflow reuse
- onboarding and teachability
- product identity

## Non-Goals

Version 1 does not include:

- character animation
- office scenes
- sprite-based actors
- a workflow editor
- arbitrary user-authored workflow graphs
- a second execution engine
- speculative or fake progress
- distributed-worker architecture changes

## Core Product Rules

### Rule 1: The Runbook Must Be Truth

Every step shown in the runbook must map to real persisted workflow facts or deterministic template rules.

If the system cannot prove a step state from source facts, it must show an explicit `limited` state instead of inventing progress.

### Rule 2: One Backbone, Many Views

The plain runbook view, cockpit summaries, future office view, and any future visual experiments must all read from the same canonical runbook model.

There must not be a separate office-mode engine.

### Rule 3: Version 1 Is Derived, Not Authored

Version 1 runbooks are built from code-defined, versioned workflow templates plus live/persisted carsinOS records.

The first version does not include user-authored templates or a visual workflow builder.

### Rule 4: No UX Interruption

Runbook must land additively in Mission Control without breaking or reordering the existing operator workflow.

Boards, Calendar, Focus, Events, Assistant, Team, Cockpit, and Strategy remain intact.

### Rule 5: No Reward Hacking

The implementation must prefer strict truth and explicit degraded states over optimistic inference.

Tests must be written to catch false-positive success, stale-state drift, missing links, and invented progress.

## Version 1 Scope

### Primary Surface

Add a new top-level Mission Control tab:

- tab id: `runbook`
- label: `Runbook`
- icon: `workflow`
- nav placement: append after `strategy`
- keyboard shortcut: `-`

Existing tab order and `1-0` shortcuts must remain unchanged.

### Feature Flag

Add a new optional module flag:

- `runbook_hub`

`runbook_hub` must follow the same runtime UX control pattern as `strategy_hub`.

When disabled:

- the `Runbook` tab is hidden
- cockpit widgets depending on runbook data are hidden
- deep links into Runbook degrade gracefully with a non-destructive notice

### Supported Workflow Kinds

Version 1 supports these canonical runbook kinds:

1. `assistant_session_run`
2. `board_card_run`
3. `scheduled_job_run`
4. `strategy_task_execution`

These four cover the current high-value carsinOS workflows without inventing a second execution model.

### Version 1 Open Points

Runbook instances must be openable from:

- Runbook tab landing page
- Strategy task detail/context
- Boards card detail/context
- Calendar job rows
- Focus items
- Assistant chat surface when a session or run exists

Cockpit integration in Version 1 is summary-only, not a second full runbook renderer.

## Canonical Model

Version 1 introduces a canonical runbook read-model family. The canonical model may be computed from existing persisted records plus code-defined templates; it does not require a user-authored workflow table in Version 1.

### Template Catalog

Runbook templates are versioned, code-defined descriptions of supported workflow kinds.

Required fields:

- `template_id`
- `template_version`
- `runbook_kind`
- `step definitions`
- `allowed transitions`
- `terminal success steps`
- `terminal failure steps`
- `waiting step definitions`
- `linkable entity kinds`

Template definitions must live in code and be unit-tested.

Each template definition must also include:

- `template_step_order`
- `branch_priority_rules`
- `action catalog`

`template_step_order` is the canonical sequence index used for tie-breaking in step selection and rendering.

`branch_priority_rules` are deterministic code-defined rules that decide which branch is considered current when multiple conditional branches are technically reachable from the same source facts.

In Version 1, a deterministic template rule is limited to:

- field-presence checks on durable records
- timestamp ordering across durable records
- explicit approval unresolved/resolved state
- explicit run/job/task terminal state
- explicit link presence or absence

It must not include probabilistic inference, speculative intent, or LLM-generated interpretation.

### Runbook Anchor

Every runbook snapshot has exactly one primary anchor:

- `assistant_session_run`: anchor by `run_id`
- `board_card_run`: anchor by `card_id`
- `scheduled_job_run`: anchor by `job_id`
- `strategy_task_execution`: anchor by `task_id`

Runbook snapshots may include linked entities beyond the primary anchor, but the anchor determines template selection and state precedence.

### Runbook Snapshot

Each runbook snapshot must expose:

- `runbook_id`
- `runbook_kind`
- `template_id`
- `template_version`
- `anchor_kind`
- `anchor_id`
- `title`
- `status`
- `status_reason`
- `generated_at_ms`
- `selected_execution_ref`
- `active_step_id`
- `next_step_ids`
- `linked_entities`
- `steps`
- `history`
- `actions`
- `data_availability`

`runbook_id` must be stable and deterministic for the same kind + anchor:

- `assistant_session_run:{run_id}`
- `board_card_run:{card_id}`
- `scheduled_job_run:{job_id}`
- `strategy_task_execution:{task_id}`

`selected_execution_ref` identifies the specific run/job-run/approval thread currently driving status for anchors that can accumulate multiple executions over time.

### Execution Selection Rules

Version 1 exposes one runbook snapshot per `(runbook_kind, anchor_id)`.

For anchor kinds that can accumulate multiple executions over time, the read model must select exactly one execution thread to drive `status`, `active_step_id`, `next_step_ids`, and summary labels.

Selection order is:

1. newest non-terminal execution for the anchor in `waiting`, `blocked`, or `active` state by durable execution timestamp
2. otherwise newest terminal execution by `finished_at_ms`
3. otherwise newest started execution by `started_at_ms`
4. otherwise anchor-only `pending` snapshot with no selected execution

Durable execution timestamp means:

- `finished_at_ms` when terminal
- else `waiting_since_ms` when waiting
- else `started_at_ms`
- else record `created_at_ms`

Per-kind durable execution timestamp sources are:

- `assistant_session_run`: selected run timestamps
- `board_card_run`: selected linked run timestamps
- `scheduled_job_run`: selected job run timestamps, falling back to linked run timestamps only when the job run lacks a durable timestamp
- `strategy_task_execution`: resolved runtime-link run timestamps, then linked job run timestamps, then task `updated_at_ms`, then task `created_at_ms`

Tie-breakers are:

1. later durable execution timestamp
2. later record `created_at_ms`
3. lexical entity id

If no `selected_execution_ref` exists, the snapshot sort timestamp must be the anchor `updated_at_ms`, falling back to anchor `created_at_ms`.

Archived anchors remain renderable and listable in Version 1 if the primary anchor record still exists.

Deleted primary anchors are not listable. Detail requests for deleted primary anchors must return an explicit `404 anchor_missing` error instead of fabricating a `limited` snapshot.

### Runbook Status

Canonical runbook status values:

- `pending`
- `active`
- `waiting`
- `blocked`
- `failed`
- `completed`
- `limited`

Status precedence must be deterministic:

1. `failed` if the selected execution has a durable terminal failure fact not superseded by a newer success fact for the same execution thread
2. `completed` if the selected execution has a durable terminal success fact and no newer non-terminal fact for the same execution thread
3. `waiting` if one or more unresolved approvals or explicit wait states exist for the selected execution
4. `blocked` if the anchor or required dependency is explicitly blocked and no higher-priority terminal/waiting state is proven
5. `active` if the selected execution is in progress and no higher-priority state is proven
6. `pending` if the anchor exists and the template applies but no execution has started
7. `limited` if the primary anchor exists but the system cannot safely resolve one of the states above because required secondary facts are missing or inconsistent

If explicit failure, completion, or waiting facts exist alongside missing secondary facts, the proven status wins and `data_availability` carries the limitation warning. The root status only becomes `limited` when no stronger status can be safely proven.

### Runbook Step

Each step must expose:

- `step_id`
- `label`
- `kind`
- `state`
- `state_reason`
- `started_at_ms`
- `finished_at_ms`
- `waiting_since_ms`
- `linked_entity_refs`
- `action_refs`
- `template_index`

`template_index` is the wire name for the canonical template step order. It must equal the template definition's `template_step_order` value.

Step state values:

- `idle`
- `active`
- `waiting`
- `blocked`
- `failed`
- `completed`
- `skipped`
- `limited`

### Active Step and Next Step Resolution

`active_step_id` must be selected deterministically after all step states are computed.

Selection order is:

1. highest-priority step state aligned to the resolved root status
2. highest `template_index` among matching steps
3. latest durable step timestamp
4. lexical `step_id`

The highest-priority step state aligned to the resolved root status is:

- `failed` for failed runbooks
- `completed` for completed runbooks
- `waiting` for waiting runbooks
- `blocked` for blocked runbooks
- `active` for active runbooks
- `idle` for pending runbooks
- `limited` for limited runbooks

If no step matches the resolved root status, fallback selection order is:

1. `active`
2. `waiting`
3. `blocked`
4. `idle`
5. `limited`
6. `completed`
7. `failed`

`next_step_ids` must be the allowed outgoing transitions from `active_step_id` whose destination steps are still reachable under current branch conditions.

`next_step_ids` must be sorted by:

1. template transition order
2. destination `template_index`
3. lexical `step_id`

If the runbook is terminal, `next_step_ids` must be empty.

### Skipped Step Rule

`skipped` is used only when a template branch becomes definitively unreachable because another durable branch condition was satisfied first.

`skipped` must not be used to hide missing data, inconsistent links, or deleted facts.

### Linked Entity Refs

Runbook linked entity refs must support:

- `task_id`
- `project_id`
- `goal_id`
- `card_id`
- `job_id`
- `job_run_id`
- `session_id`
- `run_id`
- `approval_id`
- `message_id`
- `thread_id`
- `agent_id`

Each ref must also expose:

- entity kind
- display label
- in-app deep link target

### Owner Derivation

`owner_agent_id` in list and detail read models must be derived deterministically:

1. `assistant_session_run`: run owner agent when present, otherwise session owner agent
2. `board_card_run`: board card owner agent when present, otherwise linked task owner agent, otherwise selected execution owner agent
3. `scheduled_job_run`: job owner agent when present, otherwise selected execution owner agent
4. `strategy_task_execution`: task owner agent when present, otherwise linked board card owner agent, otherwise linked job owner agent

If no durable owner can be proven, `owner_agent_id` must be omitted rather than guessed.

### Runbook Actions

Each runbook action must expose:

- `action_id`
- `action_kind`
- `label`
- `target_entity_ref`
- `availability`
- `disabled_reason`

`availability` values are:

- `enabled`
- `disabled`
- `hidden`

`action_kind` in Version 1 is limited to:

- `open_entity`
- `open_runbook`
- `resume_run`
- `open_approval`
- `run_job_now`
- `toggle_job_enabled`

The backend must only emit actions the current operator is allowed to invoke or inspect. Actions that are relevant but currently not invokable may be returned as `disabled` with a reason.

### Runbook History

Version 1 history is a normalized timeline of durable workflow facts, not a fake animation log.

Each history item must expose:

- `history_id`
- `event_kind`
- `label`
- `detail`
- `occurred_at_ms`
- `step_id`
- `entity_refs`

The API must return history items in ascending order by `occurred_at_ms`, with lexical `history_id` as the tie-breaker.

The UI may reverse that order for display.

Tests must follow API order unless they are explicitly validating the reversed UI timeline.

Only facts with a durable `occurred_at_ms` may appear in user-visible `history`.

Facts without a durable timestamp may appear in `source_facts` only.

### Source Facts and Availability

`source_facts` are diagnostic inputs used to justify the derived runbook state. They are not the primary operator timeline.

Each source fact must expose:

- `fact_id`
- `fact_kind`
- `entity_ref`
- `occurred_at_ms`
- `partial`

`data_availability` must expose:

- `is_limited`
- `is_stale`
- `last_refresh_at_ms`
- `missing_source_kinds`
- `stale_reason`

`warnings` are operator-visible guardrails that do not replace the canonical status and must expose:

- `warning_id`
- `warning_kind`
- `message`

## Data Truth and Derivation Rules

### Version 1 Principle

Version 1 prefers deterministic derivation from persisted facts over new mutable state.

New persistence is allowed only where the existing model cannot provide durable, operator-trustworthy history or identity.

### Required Source Facts

The runbook read model may derive from:

- tasks
- projects
- goals
- board cards
- jobs
- job runs
- sessions
- runs
- tool calls
- approvals
- assistant worker/task link records
- channel/runtime status when used as stale or warning context only

WebSocket events are for freshness and refresh triggers, not as the sole source of truth.

### No Inference Rule

The read model must not mark a step `completed`, `failed`, `waiting`, or `active` unless the corresponding condition is backed by durable records or a deterministic, tested template rule.

### Limited State Rule

If a workflow kind cannot be fully resolved because required linked records are missing, deleted, or inconsistent, the runbook must:

- remain renderable
- show `limited`
- explain what source facts are missing
- preserve visible linked records that do exist

This rule applies only when the primary anchor still exists.

If the primary anchor is missing, the list endpoint must omit the runbook and the detail endpoint must return `404 anchor_missing`.

Secondary linked-record failures must remain renderable as `limited` when the anchor exists.

For inconsistent link chains, the backend may still return a `limited` snapshot only when both of these are true:

- the primary anchor exists
- at least one direct linked entity ref or one durable execution record can still be proven

If neither condition is met, the detail endpoint must return an explicit user-safe `409 inconsistent_runbook_links`.

## Workflow Templates

### 1. Assistant Session Run

Anchor:

- `run_id`

Required step model:

1. `session_selected`
2. `user_input_ready`
3. `run_created`
4. `run_executing`
5. `approval_wait` (conditional)
6. `tool_activity` (conditional)
7. `memory_context` (conditional)
8. `run_succeeded` or `run_failed`

Truth sources:

- session
- run
- messages
- tool calls
- approvals
- run usage / context.why metadata when present

Execution selection:

- the run anchor is already unique, so `selected_execution_ref` must point to the same `run_id`

Blocked-state rule:

- approvals and user-input gates are always `waiting`, never `blocked`
- assistant-session runbooks do not emit root `blocked` in Version 1 unless a linked strategy task explicitly blocks the execution context

Operator actions:

- open session
- open run
- resume run when resumable
- open related approval

### 2. Board Card Run

Anchor:

- `card_id`

Required step model:

1. `card_ready`
2. `session_linked`
3. `run_created`
4. `run_executing`
5. `approval_wait` (conditional)
6. `card_run_completed` or `card_run_failed`

Truth sources:

- board card
- linked session
- latest run
- approvals for latest run

Execution selection:

- prefer the newest non-terminal linked run for the card
- otherwise use the latest terminal linked run
- if no run exists, render an anchor-only `pending` snapshot

Blocked-state rule:

- approvals are `waiting`
- card-level failures are `failed`
- `blocked` is only valid when a linked strategy task explicitly blocks the execution context

Operator actions:

- open board card
- open linked task if present
- open run
- open approval

### 3. Scheduled Job Run

Anchor:

- `job_id`

Required step model:

1. `job_enabled`
2. `job_due_or_triggered`
3. `job_run_started`
4. `run_executing` or `job_processing`
5. `approval_wait` (conditional)
6. `job_run_succeeded` or `job_run_failed`

Truth sources:

- job
- job runs
- linked runs when payload execution yields a run
- approvals associated with linked runs

Execution selection:

- prefer the newest non-terminal job run for the job
- otherwise use the latest terminal job run
- if the selected job run links to a run, that linked run drives the run-specific steps

Blocked-state rule:

- approvals are `waiting`
- job-run failures are `failed`
- disabled jobs do not count as `blocked`; they result in `pending` or `limited` depending on link completeness

Operator actions:

- open job
- run now
- toggle enabled state
- open latest job run
- open linked run if any

### 4. Strategy Task Execution

Anchor:

- `task_id`

Required step model:

1. `task_defined`
2. `execution_linked`
3. `execution_active`
4. `approval_wait` (conditional)
5. `blocked` (conditional)
6. `execution_completed` or `execution_failed`

Truth sources:

- task
- linked board card
- linked job
- latest linked run/session resolved through existing task runtime link logic
- critical approval backlog / linked approval

Execution selection:

- prefer the active runtime link resolved through existing task runtime link logic
- otherwise use the latest linked execution thread
- if only management records exist, render an anchor-only `pending` snapshot

Blocked-state rule:

- explicit task blocked state is `blocked`
- approvals remain `waiting`
- linked execution failures remain `failed`

Operator actions:

- open task
- open board card
- open job
- open run/session
- clear or update links in Strategy

## UX Spec

### Landing Page

The Runbook tab landing page must show:

- top summary strip
- recent active/waiting/blocked/failed runbooks
- filters
- primary runbook list
- detail pane for the selected runbook

The landing page is not a dashboard collage. It is a workflow-centered navigation surface.

### Summary Strip

Required summary metrics:

- active
- waiting
- blocked
- failed
- completed today

These metrics must come from the same runbook listing/read model used by the page.

`completed today` must be computed in the operator locale timezone from the terminal success timestamp.

Runbooks with missing terminal success timestamps must be excluded from `completed today` and surface a warning in detail.

If the backend exposes a summary helper endpoint for performance, that endpoint must accept an explicit timezone offset parameter and return the same result the frontend would compute locally.

### Runbook List

Each list item must show:

- title
- runbook kind
- current status
- current or blocking step label
- owner/agent when available
- primary linked entity label
- updated/occurred time

List ordering in Version 1 must be:

1. selected execution durable timestamp descending
2. root status priority: `failed`, `waiting`, `blocked`, `active`, `completed`, `pending`, `limited`
3. runbook title ascending
4. `runbook_id` ascending

Anchor-only snapshots without `selected_execution_ref` use the anchor sort timestamp defined in `Execution Selection Rules`.

Filters required in Version 1:

- kind
- status
- owner agent
- linked task/project/goal context when available
- text query

### Runbook Detail Pane

The detail pane must show:

- plain visual step flow
- current step highlight
- blocker/wait reason
- next possible steps
- linked artifacts
- action buttons
- history timeline

The visual style must be plain, readable, and honest.

Version 1 must not depend on theatrical motion.

### Plain Flow Renderer

The flow renderer must:

- render the template backbone consistently
- show branches only when they exist in the selected template
- make blocked, failed, and waiting states impossible to miss
- avoid canvas-heavy novelty when a semantic DOM layout is sufficient

Accessibility is required:

- keyboard navigable
- text labels always visible
- status not conveyed by color alone

### Deep Links

Runbook detail must deep link in-app to:

- Strategy task
- Board card
- Calendar/job surface
- Focus when approval/actionable item exists
- Assistant session/run context when available

Deep links are in-app only. No raw internal URLs or filesystem paths.

## Mission Control Integrations

### New Top-Level Tab

Add `runbook` as a new top-level tab in Mission Control.

It must be additive and must not reorder existing tabs.

### Strategy Integration

Strategy task detail/context must expose:

- `Open in Runbook`
- current runbook status chip
- current step label when runbook available

Strategy remains management truth; Runbook becomes execution-map truth for supported workflow kinds.

### Boards Integration

Board card detail/context must expose:

- `Open in Runbook` when the card has a linked session, latest run, or linked task runbook path
- current runbook status chip

Boards remain execution workspace; they do not become the runbook renderer.

### Calendar Integration

Calendar/job rows must expose:

- `Open in Runbook`
- status chip for latest derived runbook state

### Focus Integration

Focus items for approvals and run failures must expose:

- `Open in Runbook`

Focus remains the attention queue, not the workflow map.

### Assistant Integration

Assistant chat must expose:

- `Open in Runbook` for the current/latest run when available

Assistant remains conversational entry, not the runbook landing surface.

### Cockpit Integration

Version 1 cockpit integration is limited to summary widgets sourced from runbook read models.

It must not create a second bespoke runbook rendering system.

Cockpit widgets may use the optional runbook summary endpoint when it exists. Otherwise they must reuse the shared list-endpoint derivation path, not invent a cockpit-only aggregation path.

## API and Protocol Contracts

Version 1 requires a new read-model family.

### Required Endpoints

- `GET /api/v1/mission-control/runbooks`
- `GET /api/v1/mission-control/runbooks/{runbook_kind}/{anchor_id}`

Optional helper endpoint if needed for performance:

- `GET /api/v1/mission-control/runbooks/summary`

### List Contract

List response must expose:

- `generated_at_ms`
- `items`
- `counts_by_status`
- `next_cursor`

`counts_by_status` must be computed against the full filtered result set before pagination is applied.

List filters must support:

- `kind`
- `status`
- `owner_agent_id`
- `query`
- `linked_task_id`
- `linked_project_id`
- `linked_goal_id`
- `limit`
- `cursor`

Cursor semantics are mandatory:

- cursors must be opaque
- cursors must encode the full stable sort tuple of the last returned item
- changing any filter invalidates prior cursors
- invalid or stale cursors must return an explicit user-safe `400 invalid_cursor`

If `limit` is omitted, the backend must use a config-defined default.

If `limit` exceeds the config-defined maximum, the backend must clamp it to that maximum.

### Detail Contract

Detail response must expose the canonical runbook snapshot plus:

- `source_facts`
- `availability`
- `warnings`

`availability` is the wire form of `data_availability`.

The detail response must also expose:

- `owner_agent_id`
- `owner_agent_label`

### Protocol Types

Add protocol/frontend types for:

- `RunbookKind`
- `RunbookStatus`
- `RunbookStepState`
- `RunbookEntityRef`
- `RunbookHistoryItem`
- `RunbookAction`
- `RunbookStepResponse`
- `RunbookSummaryItemResponse`
- `RunbookDetailResponse`
- `ListRunbooksResponse`
- `RunbookSourceFact`
- `RunbookDataAvailability`
- `RunbookWarning`

## Storage and Backend Implementation Rules

### Backend Shape

Version 1 should prefer:

- code-defined template catalog
- deterministic gateway read-model assembly
- reuse of existing storage entities and link tables

Storage changes are allowed only if required for durable identity, ordering, or history that cannot be derived safely from existing records.

### Reuse Requirements

The implementation must reuse existing truth where possible:

- task runtime link resolution
- approval records
- job run history
- board card latest run/session linkage
- session/run records
- assistant worker link records

### Integrity Rules

The backend must fail closed on:

- invalid kind/anchor combinations
- inconsistent link chains
- stale cursor misuse

The API must return explicit user-safe errors rather than silently fabricating a fallback snapshot.

Primary-anchor deletion is a fail-closed `404 anchor_missing` condition, not a `limited` snapshot condition.

If inconsistent link chains exist but enough durable facts remain to render an operator-meaningful snapshot, the backend must return the snapshot with `warnings` and `data_availability.is_limited=true` instead of silently dropping the record.

`enough durable facts` means the primary anchor plus at least one durable execution record or one direct linked entity ref that can be rendered in the detail pane.

## Live Update Rules

Version 1 live freshness uses existing WebSocket events to trigger read-model refresh.

No new websocket contract is required unless existing events prove insufficient.

The runbook controller must refresh on relevant event families, including:

- run events
- approval events
- board card run-link updates
- job run updates
- task link updates

Refresh behavior must be debounced/configured in a dedicated runbook config module, not hardcoded inline in React components.

The controller must also mark data stale when either of these conditions is true:

- the websocket is disconnected beyond the configured stale threshold
- the last successful runbook refresh is older than the configured stale threshold

When data is stale, the UI must show a non-destructive stale indicator and trigger a refresh on:

- websocket reconnect
- window focus
- explicit operator retry

When the runbook feature is disabled, the controller must not subscribe to runbook-specific background refresh or hidden polling paths.

## Configuration Rules

All configurable thresholds and limits must live in dedicated config modules or runtime config contracts.

Examples:

- runbook list page size
- recent history item limit
- refresh debounce interval
- summary strip item limits
- stale visibility thresholds

No inline magic numbers in rendering logic.

## Rollout and Backout

### Rollout

Rollout order:

1. backend contracts and read models
2. frontend controller + plain renderer behind `runbook_hub`
3. deep links from Strategy, Boards, Calendar, Focus, Assistant
4. cockpit summary widgets if included

### Backout

If `runbook_hub` is disabled:

- existing tabs keep working
- existing links degrade to no-op notices instead of broken routes
- no source data schema should be invalidated
- runbook list/detail/summary endpoints must return `404 feature_disabled` or remain entirely unused by the frontend
- runbook background refresh must be off

Backout must not require deleting or rewriting existing task, board, approval, run, or job data.

If Version 1 adds new storage for identity, ordering, or template metadata, backout must leave those tables and indexes inert rather than requiring destructive cleanup.

If a template version is rolled back, older persisted references must remain readable. No source workflow fact may require rewrite solely because the runbook template changed.

## Testing and Quality Gates

### Backend Tests

Required coverage:

- list/detail read model for each supported runbook kind
- status precedence
- blocked-vs-waiting mapping
- limited-state behavior when linked entities are missing
- anchor-only snapshot ordering
- mixed terminal and stale non-terminal facts
- stale/invalid cursor behavior
- approval wait resolution transitions
- deep-link entity ref generation
- feature-disabled endpoint behavior

### Frontend Unit Tests

Required coverage:

- runbook controller state transitions
- filter behavior
- detail selection
- limited-state rendering
- stale-state indicator rendering
- status chip rendering
- deep-link routing behavior

### E2E Coverage

Required operator flows:

1. Assistant run -> approval wait -> resolve -> resume -> runbook updates correctly
2. Board card run -> latest run linked -> runbook opens from Boards and shows truth
3. Scheduled job failure -> Focus -> Open in Runbook -> retry path updates state
4. Strategy task with linked board/job -> Runbook opens from Strategy and shows cross-links
5. `runbook_hub` disabled -> no broken navigation, no crash, no orphaned UI
6. Anchor exists but linked records are inconsistent -> detail stays renderable as `limited` with warnings when allowed
7. Deleted primary anchor -> detail returns `anchor_missing` and list omits the runbook

### Regression Gates

At minimum:

- `npm run lint`
- `npm run typecheck`
- `npm run test:unit`
- `npm run build`
- `npm run test:e2e:core`
- project-specific release/desktop and security gates already used in this repo
- targeted Rust test runs for gateway/protocol/storage before full workspace gate

## Implementation Touchpoints

Likely frontend touchpoints:

- app shell and nav:
  - `apps/mission-control/src/app/tabs.ts`
  - `apps/mission-control/src/app/useAppController.ts`
  - `apps/mission-control/src/app/AppShell.tsx`
  - `apps/mission-control/src/app/AppContent.tsx`
  - `apps/mission-control/src/app/useKeyboardShortcuts.ts`
- new runbook feature surface:
  - `apps/mission-control/src/features/runbook/*`
  - `apps/mission-control/src/lib/api.ts`
  - `apps/mission-control/src/types.ts`
  - `apps/mission-control/src/lib/runbookConfig.ts`
  - `apps/mission-control/src/lib/opsUxConfig.ts`
- additive integrations:
  - Strategy, Boards, Calendar, Focus, Assistant, Cockpit surfaces for deep links and status chips
- tests:
  - `apps/mission-control/src/lib/api.test.ts`
  - `apps/mission-control/src/lib/opsUxConfig.test.ts`
  - new `apps/mission-control/e2e/runbook.spec.ts`

Likely backend touchpoints:

- contracts:
  - `crates/carsinos-protocol/src/lib.rs`
- gateway read models and handlers:
  - `crates/carsinos-gateway/src/main.rs`
- storage/read-model helpers:
  - `crates/carsinos-storage/src/lib.rs`
  - optional `crates/carsinos-storage/src/runbook_templates.rs`
- tests:
  - gateway route tests under `crates/carsinos-gateway/tests/`
  - storage tests for snapshot assembly and precedence

Storage work in Version 1 is optional unless derivation proves insufficient for durable, honest history.

## Explicit V1 Product Constraints

Version 1 must stay:

- plain
- trustworthy
- operator-readable
- additive
- non-theatrical
- grounded in current carsinOS workflow truth

Version 1 must not collapse into:

- a giant generic graph explorer
- a second cockpit
- a new workflow engine
- an animation project disguised as product work

## Final Recommendation

Implement the visual runbook as a new top-level, feature-flagged Mission Control layer backed by a canonical derived runbook model for four supported workflow kinds.

Ship the plain runbook first.

Make it real enough that later office or animated views can sit on top of the same backbone without any rewrite of execution truth.
