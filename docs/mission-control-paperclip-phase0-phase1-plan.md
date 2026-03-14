# Mission Control Paperclip Phase 0-1 Plan

**Generated:** 2026-03-09
**Status:** SpecSwarm folded
**Estimated Complexity:** High

## 1. Summary

Mission Control will absorb the highest-ROI Paperclip ideas without becoming Paperclip.

Phase 0 and Phase 1 add a carsinOS-native management layer exposed through a new `Strategy` tab. `Strategy` answers the top-down questions:

- what matters
- why it exists
- who owns it
- what is blocked
- what is stale
- what is costing money

Existing runtime tabs keep their jobs:

- `Boards` remains the execution surface
- `Calendar` remains the scheduling surface
- `Focus` remains the incident / approvals surface
- `Team` remains the agent roster plus hierarchy surface
- `Cockpit` remains the runtime dashboard surface

## 2. Locked Decisions

### 2.1 Product posture

- carsinOS stays the execution plane.
- Mission Control stays the operator console.
- Paperclip is reference material only for Phase 0/1.
- Do not bind Mission Control directly to vendor Paperclip APIs or schemas.
- Do not introduce multi-company, memberships, or hosted board/governance behavior.
- Do not convert Mission Control into a routed web app.

### 2.2 Navigation and UX

- Add a new top-level tab named `Strategy`.
- Append `Strategy` after `Cockpit` so current tab order does not shift.
- Keep the existing `1-9` shortcuts unchanged.
- Assign `0` as the `Strategy` shortcut.
- Use the already-imported `Compass` icon for the `Strategy` tab.
- `Strategy` is additive. Existing workflows in `Boards`, `Calendar`, `Focus`, `Team`, and `Cockpit` must continue to work without visiting `Strategy`.
- All `deep-links` in this spec mean in-app state transitions only. They may switch tabs, apply filters, and select records, but they must not introduce URL routing.

### 2.3 Canonical work model

- `Goals`, `projects`, and `tasks` become the canonical management records.
- `Board cards`, `jobs`, and `runs` are linked execution artifacts, not the source of truth.
- Existing boards remain in place and are not migrated away in Phase 1.
- Existing board cards can be linked to new tasks incrementally.
- When task state and board state disagree, task state wins for management reporting and summary views. Board/card state continues to control board rendering and execution affordances only.
- No automatic sync will rewrite board columns from task status in Phase 1.

### 2.4 Hierarchy scope

- Extend agents with `reports_to_agent_id` and `role_label`.
- Phase 1 hierarchy is `light helpers` only.
- Allowed hierarchy behavior:
  - org chart / reporting line display
  - manager chain display in task and focus context
  - suggested owner filters in `Strategy` and `Team`
  - subtree filtering in `Strategy` and `Team`
- Forbidden hierarchy behavior in Phase 1:
  - no automatic routing
  - no automatic escalation
  - no approval chain changes
  - no tool policy changes

### 2.5 Bootstrap scope

- Reuse the existing onboarding wizard and `Team` flows.
- Add reusable `bootstrap presets` for worker roles.
- Presets are local-first and exportable/importable as small JSON files.
- Presets contain only setup defaults, not full operation-pack state.
- Do not ship full operation packs in Phase 1.

## 3. No-Touch Boundaries

- Do not replatform Mission Control into Paperclip.
- Do not add SaaS tenancy, memberships, or company switching.
- Do not replace `Boards` with `Strategy`.
- Do not add hierarchy-driven runtime semantics.
- Do not add a separate onboarding product or wizard.
- Do not add a full portable environment/export bundle.

## 4. Global Contract Rules

### 4.1 Time policy

- All timestamps are Unix epoch milliseconds in UTC.
- This applies to every `*_at` field, plus `due_at`, `target_date`, and `generated_at_ms`.
- `today` in strategy summary uses the operator timezone day boundary and the same query contract as Mission Control usage:
  - `timezone`
  - `tz_offset_minutes`

### 4.2 Create / update / archive policy

- `POST /collection` creates a new record.
- `POST /collection/{id}` is a partial update.
- Omitted fields stay unchanged.
- Partial updates are idempotent by resulting state.
- Phase 1 does not add hard-delete endpoints for goals, projects, or tasks.
- Archival is done through `status = "archived"`.
- `archived` records are hidden by default and shown only by explicit filter.

### 4.3 Status transition policy

- `blocked_reason` is required when the resulting task status is `blocked`.
- Moving a task away from `blocked` clears `blocked_reason`.
- Moving a task from `blocked` to `done` must clear `blocked_reason` in the same update.
- Archived tasks may be reopened only by an explicit status update from `archived` to another valid status.
- `done` and `archived` count as closed states for project completion.
- `todo`, `in_progress`, and `blocked` count as open states for project completion.

### 4.4 List endpoint policy

Every new list endpoint must support:

- `limit`
- `cursor`
- `sort`

Defaults:

- `limit=50`
- `max_limit=200`
- default sort is `updated_at_desc`

Common filters:

- `status`
- `owner_agent_id`
- `query`

Task-specific filters:

- `goal_id`
- `project_id`
- `stale`
- `blocked`
- `unassigned`
- `hierarchy_root_agent_id`
- `hierarchy_scope=subtree`

### 4.5 Compatibility / partial availability policy

- `Strategy` must tolerate backend versions that do not yet expose all management endpoints.
- If CRUD endpoints or summary endpoints are missing, the UI shows a non-blocking empty state explaining that the connected gateway does not yet expose the Strategy surface.
- Existing tabs must continue rendering normally.
- Missing summary data must never crash the page.

## 5. Data Model and Contracts

### 5.1 Goal

Add a new goal entity with this contract:

- `goal_id: string`
- `slug: string`
- `title: string`
- `summary: string`
- `status: "active" | "at_risk" | "completed" | "archived"`
- `owner_agent_id: string | null`
- `target_date: number | null`
- `progress_pct: number`
- `created_at: number`
- `updated_at: number`

Rules:

- `progress_pct` is read-only and derived.
- `progress_pct` is `0..100`.
- `slug` is unique case-insensitively within the local gateway.
- allowed slug charset is `[a-z0-9-]`
- slug length is `3..64`
- slug collisions return `409`
- goal progress is computed from non-archived leaf tasks under that goal:
  - numerator: leaf tasks with `status = "done"`
  - denominator: leaf tasks with `status != "archived"`
  - if denominator is `0`, progress is `0`
  - result is rounded to the nearest whole integer

### 5.2 Project

Add a new project entity with this contract:

- `project_id: string`
- `goal_id: string`
- `slug: string`
- `name: string`
- `summary: string`
- `status: "active" | "blocked" | "completed" | "archived"`
- `owner_agent_id: string | null`
- `workspace_root: string | null`
- `budget_month_usd: number | null`
- `created_at: number`
- `updated_at: number`

Rules:

- each project belongs to exactly one goal in Phase 1
- `budget_month_usd` is optional, non-negative, and stored in USD
- `budget_month_usd` uses at most two decimal places
- `workspace_root` may be `null`, `.` , or an absolute path
- other relative paths are invalid
- a project cannot be `completed` while it has open tasks

### 5.3 Task

Add a new task entity with this contract:

- `task_id: string`
- `project_id: string`
- `parent_task_id: string | null`
- `title: string`
- `detail: string`
- `status: "todo" | "in_progress" | "blocked" | "done" | "archived"`
- `priority: "low" | "normal" | "high" | "critical"`
- `owner_agent_id: string | null`
- `due_at: number | null`
- `blocked_reason: string | null`
- `linked_board_card_id: string | null`
- `linked_job_id: string | null`
- `latest_run_id: string | null`
- `latest_session_id: string | null`
- `created_at: number`
- `updated_at: number`

Rules:

- a task belongs to exactly one project
- parent/child tasks must stay within the same project
- `blocked_reason` is required when `status = "blocked"`
- tasks are `stale` when `status` is not `done` or `archived` and `updated_at` is older than `72h`
- `updated_at` changes only on direct task record mutations and link target changes
- `updated_at` does not change when only `latest_run_id` or `latest_session_id` refreshes
- `latest_run_id` and `latest_session_id` are derived read-model fields
- `latest_run_id` and `latest_session_id` update when:
  - a task is first linked to a board card or job and that target already has a latest run/session
  - a new run starts or completes for the linked board card or linked job
- unrelated runs must not update these fields

### 5.4 Linking rules

- One task may link to at most one board card and at most one job.
- One board card may link to at most one task.
- One job may link to at most one task.
- Linking does not create the board card or job automatically.
- Linking fails with `404` when the target does not exist.
- Linking fails with `409` when the target is already linked elsewhere.
- Reassignment is allowed only with `force_reassign=true`.
- Reassignment clears the previous task link atomically before creating the new one.

### 5.5 Agent hierarchy

Extend the existing agent contract with:

- `reports_to_agent_id: string | null`
- `role_label: string | null`

Rules:

- `reports_to_agent_id` must reference an existing agent or be null
- self-reference is invalid
- hierarchy cycles are invalid
- hierarchy validation runs on the affected subtree only
- removing an agent must fail while other agents, goals, projects, or tasks still reference it
- operator must reassign or clear those references before removal

### 5.6 Bootstrap preset

Add a bootstrap preset contract with:

- `schema_version: "mc-bootstrap-preset-v1"`
- `preset_key: string`
- `display_name: string`
- `description: string`
- `role_label: string`
- `provider_path: "openai" | "anthropic" | "local"`
- `default_model_provider: string | null`
- `default_model_id: string | null`
- `default_tool_profile: string | null`
- `default_workspace_root: string | null`
- `setup_notes: string | null`
- `created_at: number`
- `updated_at: number`

Rules:

- presets do not store secrets
- presets do not store gateway tokens
- presets do not store auth profile ids
- presets do not store manager agent ids
- manager selection happens when the preset is applied
- `provider_path` controls onboarding/setup lane
- `default_model_provider` controls runtime defaults and must be compatible with `provider_path`
- compatibility rules:
  - `provider_path = "openai"` requires `default_model_provider = "openai"` when set
  - `provider_path = "anthropic"` requires `default_model_provider = "anthropic"` when set
  - `provider_path = "local"` may use any locally supported provider surfaced by the existing onboarding local-provider catalog
- import/export payload size limit is `16 KB`
- import rejects unknown top-level keys
- import rejects colliding `preset_key` values unless `overwrite=true`
- import rejects any key containing:
  - `token`
  - `secret`
  - `password`
  - `api_key`
  - `access_token`
  - `refresh_token`
- import rejects any string value that starts with:
  - `Bearer `
  - `sk-`
  - `sess-`

## 6. API and Read Models

### 6.1 New CRUD endpoints

Add carsinOS-native HTTP contracts for:

- `GET /api/v1/goals`
- `POST /api/v1/goals`
- `GET /api/v1/goals/{goal_id}`
- `POST /api/v1/goals/{goal_id}`
- `GET /api/v1/projects`
- `POST /api/v1/projects`
- `GET /api/v1/projects/{project_id}`
- `POST /api/v1/projects/{project_id}`
- `GET /api/v1/tasks`
- `POST /api/v1/tasks`
- `GET /api/v1/tasks/{task_id}`
- `POST /api/v1/tasks/{task_id}`

Update semantics for all `POST /{id}` routes:

- partial update only
- omitted fields unchanged
- return the full updated record
- validate the resulting record, not just the changed fields

### 6.2 Linking endpoints

Add explicit task linking mutations for:

- `POST /api/v1/tasks/{task_id}/links/board-card`
- `POST /api/v1/tasks/{task_id}/links/job`
- `POST /api/v1/tasks/{task_id}/links/clear`

### 6.3 Agent contract updates

Extend:

- `GET /api/v1/agents`
- `GET /api/v1/agents/{agent_id}`
- `POST /api/v1/agents/{agent_id}`
- `POST /api/v1/agents`

to include:

- `reports_to_agent_id`
- `role_label`

### 6.4 Management summary read model

Add a new read model:

- `GET /api/v1/mission-control/strategy/summary?timezone={tz}&tz_offset_minutes={n}`

Response shape:

- `generated_at_ms: number`
- `currency: "USD"`
- `blocked_task_count: number`
- `blocked_tasks: StrategyTaskListItem[]`
- `stale_task_count: number`
- `stale_tasks: StrategyTaskListItem[]`
- `spend_by_agent: StrategySpendByAgentItem[]`
- `spend_by_project: StrategySpendByProjectItem[]`
- `unattributed_spend_total: number`
- `goal_progress: StrategyGoalProgressItem[]`
- `critical_approval_backlog_count: number`
- `critical_approval_backlog: StrategyApprovalBacklogItem[]`

Projection shapes:

- `StrategyTaskListItem`
  - `task_id`
  - `title`
  - `status`
  - `priority`
  - `owner_agent_id`
  - `owner_name`
  - `project_id`
  - `project_name`
  - `goal_id`
  - `goal_title`
  - `updated_at`
  - `due_at`
  - `blocked_reason`
- `StrategySpendByAgentItem`
  - `agent_id`
  - `agent_name`
  - `estimated_cost_total`
  - `linked_task_count`
- `StrategySpendByProjectItem`
  - `project_id`
  - `project_name`
  - `goal_id`
  - `goal_title`
  - `estimated_cost_total`
  - `attributed_run_count`
- `StrategyGoalProgressItem`
  - `goal_id`
  - `title`
  - `progress_pct`
  - `open_task_count`
  - `blocked_task_count`
- `StrategyApprovalBacklogItem`
  - `approval_id`
  - `kind`
  - `summary`
  - `linked_task_id`
  - `requested_at`

Summary list rules:

- `blocked_tasks`, `stale_tasks`, and `critical_approval_backlog` return top `20` items only
- the corresponding `*_count` field returns the full total
- default order is `updated_at desc` for task lists and `requested_at desc` for approval backlog

Summary data sources:

- approval backlog derives from existing requested approvals
- spend by agent derives from current Mission Control usage data
- spend by project derives only from linked runs/jobs/tasks
- missing links roll cost into `unattributed_spend_total`
- `critical_approval_backlog` includes requested approvals that are either:
  - linked to a `critical` priority task
  - or older than `30m`

### 6.5 Bootstrap preset endpoints

Add:

- `GET /api/v1/bootstrap-presets`
- `POST /api/v1/bootstrap-presets`
- `GET /api/v1/bootstrap-presets/{preset_key}`
- `POST /api/v1/bootstrap-presets/{preset_key}`
- `POST /api/v1/bootstrap-presets/{preset_key}/export`
- `POST /api/v1/bootstrap-presets/import`

## 7. Mission Control UX

### 7.1 Strategy tab

Add a new `Strategy` feature surface with three vertical zones:

- top summary strip
- left navigator
- main detail canvas

#### Top summary strip

Show six cards:

- blocked tasks
- stale tasks
- spend today by agent
- spend today by project
- goal progress
- critical approvals

Each card must deep-link to the relevant filtered view in `Strategy` or `Focus`.

#### Left navigator

Provide:

- goal list with status and progress
- per-goal project counts
- filters for `all`, `blocked`, `stale`, `assigned`, `unassigned`
- owner filter
- manager/team filter derived from full hierarchy subtree

#### Main detail canvas

Use a split view:

- task list on the left
- selected task detail on the right

Task detail includes:

- title
- detail
- status
- priority
- owner
- manager chain
- due date
- blocked reason
- linked board card
- linked job
- latest run
- latest session
- parent task / child tasks

Allow CRUD directly in this surface for goals, projects, and tasks.

### 7.2 Existing tab integration

#### Boards

- add read-only task linkage context to card detail
- add an in-app deep-link from card detail to the linked task
- allow linking an existing board card to an existing task from the card detail drawer
- do not change drag/drop behavior

#### Calendar

- show linked task title in job detail rows when a job is linked
- add an in-app deep-link from job to task
- do not change scheduler controls

#### Focus

- show task / project / goal context when an approval or blocker maps to a linked task
- show owner + manager chain summary
- do not change approval actions

#### Team

- add `role label`
- add `reports to`
- add org view toggle with manager/subordinate grouping
- add hierarchy-aware owner suggestion chips for open tasks
- keep current edit flow in place

#### Cockpit

Add new built-in widgets:

- `strategy_summary`
- `blocked_work`
- `stale_work`
- `goal_progress`
- `project_spend`
- `approval_backlog`

Widget rollout rules:

- new widgets are not auto-pinned into existing layouts
- existing user layouts must stay unchanged
- when `strategy_hub=false`, hidden management widgets are skipped during render even if still present in saved layout data

### 7.3 Onboarding and bootstrap

Reuse the existing onboarding wizard and `Team` create/edit modal.

Add preset support in both places:

- choose preset
- preview preset defaults
- apply preset to the agent draft
- optionally save current draft as a preset
- export/import preset JSON

Phase 1 bootstrap goals:

- faster worker setup
- consistent role labels
- consistent provider/model defaults
- consistent workspace defaults

Phase 1 non-goals:

- remote invites
- cross-machine trust exchange
- external worker enrollment

## 8. Implementation Touchpoints Map

### 8.1 App shell and tab registration

- `apps/mission-control/src/app/AppShell.tsx`
- `apps/mission-control/src/app/AppContent.tsx`
- `apps/mission-control/src/app/tabs.ts`
- `apps/mission-control/src/app/useAppController.ts`
- `apps/mission-control/src/app/useKeyboardShortcuts.ts`

### 8.2 Frontend strategy feature and shared styling

- `apps/mission-control/src/features/strategy/*`
- `apps/mission-control/src/styles.css`
- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/types.ts`

### 8.3 Existing-tab additive surfaces

- `apps/mission-control/src/features/boards/*`
- `apps/mission-control/src/features/calendar/*`
- `apps/mission-control/src/features/focus/*`
- `apps/mission-control/src/features/team/*`
- `apps/mission-control/src/features/cockpit/*`
- `apps/mission-control/src/features/onboarding/*`

### 8.4 Backend contracts and persistence

- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-storage/src/lib.rs`
- `crates/carsinos-gateway/src/main.rs`

### 8.5 Rollout controls

- `apps/mission-control/src/lib/opsUxConfig.ts`
- `apps/mission-control/src/app/AppContent.tsx`
- cockpit widget registry / renderer files under `apps/mission-control/src/features/cockpit/*`

## 9. Rollout and Backout

### 9.1 Rollout

Add a new Mission Control ops UX control:

- `strategy_hub`

Behavior:

- default `true`
- when `false`, hide the `Strategy` tab
- when `false`, hide new management cockpit widgets
- when `false`, hide preset import/export/apply UI in `Team` and onboarding
- existing linked task data may still exist in the backend
- existing preset data may still exist in the backend
- existing tabs must continue to render without task context when management endpoints are unavailable

### 9.2 Backout

If rollout must be reversed:

1. disable `strategy_hub`
2. keep the backend records intact
3. preserve links and presets in storage
4. leave existing user cockpit layouts untouched
5. suppress hidden management widgets during render instead of deleting them from saved layout state
6. revert UI exposure first, not the data model

## 10. Implementation Plan

### Phase 0: Framing + scaffolding

**Goal:** add the management vocabulary and UI seams without interrupting the current operator flow.

#### Task 0.1: Add the Strategy tab shell

- **Location:** `apps/mission-control/src/app/*`
- **Description:** extend tab types, tab config, app shell, help banner hooks, and app content mounting for the new `Strategy` tab
- **Complexity:** 4
- **Dependencies:** none
- **Acceptance Criteria:**
  - `Strategy` appears after `Cockpit`
  - `1-9` shortcuts remain unchanged
  - `0` opens `Strategy`
  - partial backend availability renders non-blocking empty state instead of crashing
- **Validation:**
  - unit coverage for tab registration and shortcut behavior

#### Task 0.2: Add the frontend controller and feature flag seam

- **Location:** `apps/mission-control/src/app/*`, `apps/mission-control/src/lib/opsUxConfig*`
- **Description:** create a strategy controller boundary plus `strategy_hub` rollout control
- **Complexity:** 5
- **Dependencies:** Task 0.1
- **Acceptance Criteria:**
  - `Strategy` can be hidden without affecting existing tabs
  - controller handles loading, error, empty, and partial-availability states
- **Validation:**
  - unit tests for hidden/visible states

#### Task 0.3: Update product language across the app

- **Location:** `apps/mission-control/src/app/*`, `apps/mission-control/src/features/help/*`, `apps/mission-control/src/features/onboarding/*`, `apps/mission-control/src/features/assistant/*`
- **Description:** update guided tour copy, help content, and assistant guidance so `Strategy` is the management layer and `Boards` remain execution
- **Complexity:** 3
- **Dependencies:** Task 0.1
- **Acceptance Criteria:**
  - no copy suggests `Boards` are the only work model
  - no copy suggests Mission Control is becoming Paperclip
- **Validation:**
  - manual copy review
  - targeted unit snapshots where coverage exists

### Phase 1: Gateway model and read models

**Goal:** add the management entities, read models, and task-to-runtime linkage.

#### Task 1.1: Add protocol contracts

- **Location:** `crates/carsinos-protocol/src/lib.rs`
- **Description:** define goal, project, task, summary, link, preset, and extended agent contracts
- **Complexity:** 7
- **Dependencies:** none
- **Acceptance Criteria:**
  - all new response/request shapes are defined once in protocol
  - all timestamps are explicitly ms epoch UTC
  - agent contract includes `reports_to_agent_id` and `role_label`
- **Validation:**
  - targeted protocol tests / serde round-trip tests

#### Task 1.2: Add storage schema and queries

- **Location:** `crates/carsinos-storage/src/lib.rs`
- **Description:** add persistence for goals, projects, tasks, links, presets, and hierarchy validation
- **Complexity:** 8
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - CRUD works for all new entities
  - task parent rules and hierarchy cycle rules are enforced
  - link exclusivity and forced reassignment work atomically
  - stale-task query exists
- **Validation:**
  - storage unit tests for CRUD, cycle rejection, stale detection, link validation, reassign conflicts

#### Task 1.3: Add gateway routes and management summary read model

- **Location:** `crates/carsinos-gateway/src/main.rs`
- **Description:** expose CRUD, filtering, pagination, linking, preset import/export, and `strategy/summary` routes
- **Complexity:** 8
- **Dependencies:** Task 1.2
- **Acceptance Criteria:**
  - routes exist and match this spec exactly
  - summary aggregates blocked/stale/spend/progress/approvals
  - partial endpoint availability fails soft in the client contract
- **Validation:**
  - gateway API tests

### Phase 1: Mission Control UI

**Goal:** expose the new model cleanly without interrupting runtime operations.

#### Task 1.4: Add frontend types and API wrappers

- **Location:** `apps/mission-control/src/types.ts`, `apps/mission-control/src/lib/api.ts`
- **Description:** wire all new contracts and wrappers into the thin client
- **Complexity:** 6
- **Dependencies:** Task 1.3
- **Acceptance Criteria:**
  - no component uses raw `fetch`
  - wrappers exist for every new route in this spec
  - wrappers support timezone query for strategy summary
- **Validation:**
  - TypeScript build
  - targeted API wrapper tests

#### Task 1.5: Build the Strategy tab

- **Location:** `apps/mission-control/src/features/strategy/*`, `apps/mission-control/src/styles.css`
- **Description:** implement summary strip, goal/project navigator, task list, task detail, CRUD flows, filters, and in-app deep-links
- **Complexity:** 8
- **Dependencies:** Tasks 0.1, 0.2, 1.4
- **Acceptance Criteria:**
  - operator can create goal -> project -> task from the UI
  - operator can edit owner, status, priority, due date, links, and hierarchy context
  - empty/error/loading states are complete
  - summary cards and deep-links do not rely on URL routing
- **Validation:**
  - component tests
  - e2e flow for create/edit/link

#### Task 1.6: Add context to existing tabs

- **Location:** `apps/mission-control/src/features/boards/*`, `apps/mission-control/src/features/calendar/*`, `apps/mission-control/src/features/focus/*`, `apps/mission-control/src/features/team/*`, `apps/mission-control/src/features/cockpit/*`, `apps/mission-control/src/styles.css`
- **Description:** add task, goal, project, owner, and manager context without changing the primary purpose of those tabs
- **Complexity:** 7
- **Dependencies:** Tasks 1.4, 1.5
- **Acceptance Criteria:**
  - boards, calendar, focus, team, and cockpit show linked context where present
  - existing interactions still behave the same
  - cockpit supports the new management widgets without altering existing layouts
- **Validation:**
  - unit tests
  - e2e spot checks in each touched tab

#### Task 1.7: Extend onboarding and Team with bootstrap presets

- **Location:** `apps/mission-control/src/features/onboarding/*`, `apps/mission-control/src/features/team/*`, `apps/mission-control/src/styles.css`
- **Description:** let operators apply, save, export, and import bootstrap presets from existing setup flows
- **Complexity:** 6
- **Dependencies:** Tasks 1.4, 1.6
- **Acceptance Criteria:**
  - a preset can be applied to an agent draft
  - a current draft can be saved as a preset
  - preset import rejects unknown keys, oversized payloads, key collisions without overwrite, and secret-shaped fields
- **Validation:**
  - unit tests for preset flows
  - e2e create-agent-from-preset flow

### Phase 1: Validation and PR flow

**Goal:** land the work safely and according to repo process.

#### Task 1.8: Run required gates and acceptance checks

- **Location:** `apps/mission-control`, workspace root
- **Description:** run the Mission Control validation stack plus targeted Rust tests
- **Complexity:** 4
- **Dependencies:** Tasks 1.1 through 1.7
- **Acceptance Criteria:**
  - `npm run lint`
  - `npm run typecheck`
  - `npm run test:unit`
  - `npm run build`
  - targeted `cargo test -p carsinos-gateway -p carsinos-protocol -p carsinos-storage`
- **Validation:**
  - command output captured in checkpoint notes

#### Task 1.9: Follow PR review flow

- **Location:** git/GitHub workflow
- **Description:** open PR, request `@coderabbitai review`, wait for completion, address findings, rerun gates, then merge
- **Complexity:** 3
- **Dependencies:** Task 1.8
- **Acceptance Criteria:**
  - no merge before CodeRabbit completes
  - post-review checkpoint captures findings and revalidation
- **Validation:**
  - PR URL and review evidence in checkpoints

## 11. Test Scenarios

- create a goal, project, and task entirely in `Strategy`
- create a task with no owner, then assign an owner from hierarchy-aware suggestions
- link a task to an existing board card
- attempt to link that same board card to a second task and verify `409`
- force-reassign a board-card link and verify the old task link is cleared atomically
- link a task to an existing job
- verify `latest_run_id` updates through linked runtime activity without changing task `updated_at`
- mark a task `blocked` and require a blocked reason
- move a blocked task to `done` and verify `blocked_reason` clears
- let a task age past `72h` and verify it appears in stale work
- verify goal progress updates from non-archived leaf tasks only
- verify unattributed spend is surfaced when runs/jobs are unlinked
- view linked task context from `Boards`, `Calendar`, and `Focus`
- import a bootstrap preset and use it in onboarding
- import an oversized or colliding preset and verify rejection behavior
- export a bootstrap preset and confirm no secrets are present
- disable `strategy_hub` and verify existing tabs still work
- disable `strategy_hub` with hidden management widgets in saved cockpit layout and verify render remains stable

## 12. Rollout Risks and Mitigations

- **Risk:** `Strategy` competes with `Boards` instead of clarifying it.
  - **Mitigation:** keep copy and in-app deep-links explicit: strategy manages intent, boards execute work.
- **Risk:** hierarchy gets interpreted as runtime policy.
  - **Mitigation:** keep all hierarchy effects read-only or suggestion-only.
- **Risk:** spend-by-project is inconsistent when tasks are unlinked.
  - **Mitigation:** expose `unattributed_spend_total` in the summary contract.
- **Risk:** preset export drifts toward operation packs.
  - **Mitigation:** keep export schema narrowly scoped to worker setup defaults only.
- **Risk:** UI/backend version skew causes Strategy to fail hard.
  - **Mitigation:** enforce partial-availability fallback and non-blocking empty states.

## 13. Assumptions

- `Strategy` is the final tab name for Phase 1.
- `0` is acceptable as the new keyboard shortcut.
- single-goal-per-project is acceptable in Phase 1.
- `72h` is the stale threshold for tasks in Phase 1.
- `strategy_hub` defaults to enabled.
- unrelated local changes remain untouched:
  - `scripts/one_click_launch.command`
  - untracked `docs/PAPERCLIP_CARSINOS_STRATEGY_ASSESSMENT.md`
