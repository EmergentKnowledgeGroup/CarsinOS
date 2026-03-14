# Mission Control Paperclip Phase 0-1 Execution Checklist

Date: 2026-03-09  
Source spec: [mission-control-paperclip-phase0-phase1-plan.md](./mission-control-paperclip-phase0-phase1-plan.md)

## Purpose

Execution checklist for building the Mission Control Phase 0 and Phase 1 management layer from the folded spec.

Use this checklist to drive implementation order, verify dependencies, and keep validation and PR workflow aligned with repo SOP.

## Operating Rules

1. Keep carsinOS as the execution plane and Mission Control as the operator console.
2. Do not introduce routed web-app behavior. All `deep-links` are in-app state transitions only.
3. Do not replace `Boards`; `Strategy` is additive.
4. Keep hierarchy behavior suggestion-only in Phase 1.
5. Do not add multi-company, memberships, remote invites, or operation-pack scope.
6. Update `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json` at phase start, post-green tests, PR open, and post-merge.
7. Before each major edit block and after each meaningful milestone, run the context checkpoint snapshot command.
8. Do not merge before `@coderabbitai review` completes and findings are handled.

## Entry Criteria

- [ ] Folded spec is accepted as source of truth.
- [ ] Working branch and head are verified.
- [ ] Existing dirty changes outside this scope are identified and left untouched.
- [ ] Implementation owner knows the required validation stack:
  - `npm run lint`
  - `npm run typecheck`
  - `npm run test:unit`
  - `npm run build`
  - `cargo test -p carsinos-gateway -p carsinos-protocol -p carsinos-storage`

## Batch 0: Execution Prep

- [ ] Create implementation branch with `codex/` prefix.
- [ ] Write phase-start checkpoint for implementation.
- [ ] Re-read the folded spec and this checklist before code changes.
- [ ] Confirm current app-shell tab order and keyboard shortcut behavior.
- [ ] Confirm existing ops rollout controls and cockpit widget registry seams.

## Batch 1: Protocol and Storage Foundation

### Goal

Land the backend contracts and persistence rules that make the management layer decision-complete and safe.

### Checklist

- [ ] Add protocol types for goals, projects, tasks, task summary projections, strategy summary projections, bootstrap presets, and extended agents.
- [ ] Mark all timestamps as Unix epoch milliseconds in UTC.
- [ ] Encode derived `goal.progress_pct` semantics in protocol comments/tests.
- [ ] Add storage schema for goals.
- [ ] Add storage schema for projects.
- [ ] Add storage schema for tasks.
- [ ] Add storage schema for bootstrap presets.
- [ ] Add task parent-child validation within project boundary.
- [ ] Add slug normalization and uniqueness rules.
- [ ] Add one-to-one board-card link enforcement.
- [ ] Add one-to-one job link enforcement.
- [ ] Add `force_reassign=true` atomic reassignment behavior.
- [ ] Add hierarchy cycle rejection.
- [ ] Add hierarchy delete guards for referenced agents.
- [ ] Add stale-task query using `72h` threshold.
- [ ] Add tests for CRUD, hierarchy validation, link conflicts, force reassignment, stale detection, and slug collisions.

### Exit Criteria

- [ ] Protocol compile/test surface is green.
- [ ] Storage tests cover CRUD, links, hierarchy, and stale logic.

## Batch 2: Gateway Routes and Strategy Read Model

### Goal

Expose the management model cleanly through carsinOS-native API contracts and a typed Mission Control summary endpoint.

### Checklist

- [ ] Add list/create/get/update routes for goals.
- [ ] Add list/create/get/update routes for projects.
- [ ] Add list/create/get/update routes for tasks.
- [ ] Add list/create/get/update routes for bootstrap presets.
- [ ] Add task link board-card route.
- [ ] Add task link job route.
- [ ] Add task clear-links route.
- [ ] Extend agent routes with `reports_to_agent_id` and `role_label`.
- [ ] Add list endpoint support for `limit`, `cursor`, and `sort`.
- [ ] Add task filters for `goal_id`, `project_id`, `stale`, `blocked`, `unassigned`, and hierarchy subtree.
- [ ] Add `strategy/summary` route with `timezone` and `tz_offset_minutes`.
- [ ] Type the summary payload projections fully.
- [ ] Enforce top-20 truncation plus total counts for blocked, stale, and approval backlog lists.
- [ ] Surface `unattributed_spend_total` explicitly.
- [ ] Define critical approval backlog rule in code/tests.
- [ ] Make partial endpoint availability fail soft at the contract boundary.
- [ ] Add gateway tests for pagination/filtering, summary typing, link conflicts, and timezone/day-boundary behavior.

### Exit Criteria

- [ ] Gateway routes match the folded spec exactly.
- [ ] Summary endpoint can drive Strategy without ad-hoc client guesses.

## Batch 3: Mission Control App Shell and Strategy Surface

### Goal

Introduce `Strategy` without breaking the current operator flow.

### Checklist

- [ ] Extend app tab types with `Strategy`.
- [ ] Append `Strategy` after `Cockpit`.
- [ ] Preserve `1-9` shortcuts.
- [ ] Wire `0` to `Strategy`.
- [ ] Reuse `Compass` icon in nav rail.
- [ ] Add `strategy_hub` rollout control.
- [ ] Add Strategy controller boundary.
- [ ] Add partial-availability fallback state for missing backend support.
- [ ] Create `features/strategy` surface.
- [ ] Build summary strip with six cards.
- [ ] Build goal/project navigator.
- [ ] Build task list/detail split view.
- [ ] Add goal CRUD.
- [ ] Add project CRUD.
- [ ] Add task CRUD.
- [ ] Add task filters for blocked/stale/assigned/unassigned/subtree.
- [ ] Add in-app deep-link behavior from Strategy cards to filtered views.
- [ ] Add empty/loading/error states.
- [ ] Update guided tour copy, help text, and assistant guidance to reflect `Strategy = management` and `Boards = execution`.

### Exit Criteria

- [ ] Operator can create goal -> project -> task entirely in Strategy.
- [ ] Strategy never relies on URL routing.
- [ ] Existing tabs remain functional without using Strategy.

## Batch 4: Existing Tab Integration

### Goal

Expose management context where it helps while preserving each tab’s current role.

### Checklist

- [ ] Add task linkage context to Boards card detail.
- [ ] Add in-app deep-link from board card detail to linked task.
- [ ] Add board-card-to-task linking affordance.
- [ ] Add linked task title in Calendar job detail.
- [ ] Add in-app deep-link from job detail to linked task.
- [ ] Add task/project/goal context in Focus when available.
- [ ] Add owner + manager chain context in Focus.
- [ ] Extend Team agent editing with `role_label`.
- [ ] Extend Team agent editing with `reports_to_agent_id`.
- [ ] Add Team org-view toggle.
- [ ] Add hierarchy-aware owner suggestion chips.
- [ ] Add Cockpit widgets:
  - `strategy_summary`
  - `blocked_work`
  - `stale_work`
  - `goal_progress`
  - `project_spend`
  - `approval_backlog`
- [ ] Ensure new widgets are not auto-pinned into existing layouts.
- [ ] Ensure hidden management widgets are skipped cleanly when `strategy_hub=false`.

### Exit Criteria

- [ ] Boards, Calendar, Focus, Team, and Cockpit gain context without behavior regression.
- [ ] Existing user cockpit layouts remain stable.

## Batch 5: Bootstrap Presets

### Goal

Reuse existing onboarding and Team flows for repeatable local-first worker setup.

### Checklist

- [ ] Add bootstrap preset type/wrapper support in frontend.
- [ ] Add preset picker in onboarding.
- [ ] Add preset picker in Team create/edit flow.
- [ ] Add preset preview state.
- [ ] Add apply-preset-to-draft behavior.
- [ ] Add save-current-draft-as-preset behavior.
- [ ] Add preset export flow.
- [ ] Add preset import flow.
- [ ] Reject unknown keys on import.
- [ ] Reject secret-shaped keys/values on import.
- [ ] Reject oversized payloads on import.
- [ ] Reject colliding `preset_key` unless overwrite is explicitly requested.
- [ ] Hide preset UI when `strategy_hub=false`.

### Exit Criteria

- [ ] Operator can create an agent from a preset in existing flows.
- [ ] Preset import/export stays within setup-default scope only.

## Batch 6: Validation

### Goal

Run the required local gates before PR creation.

### Checklist

- [ ] Run `npm run lint` in `apps/mission-control`.
- [ ] Run `npm run typecheck` in `apps/mission-control`.
- [ ] Run `npm run test:unit` in `apps/mission-control`.
- [ ] Run `npm run build` in `apps/mission-control`.
- [ ] Run targeted `cargo test -p carsinos-gateway -p carsinos-protocol -p carsinos-storage`.
- [ ] Write post-green checkpoint with command evidence and any residual failures.

### Exit Criteria

- [ ] All required local gates are green or explicitly documented as blocked.

## Batch 7: PR and Review Flow

### Goal

Complete the mandatory PR workflow without skipping review or revalidation.

### Checklist

- [ ] Open PR.
- [ ] Write `PR open` checkpoint with PR URL.
- [ ] Request `@coderabbitai review`.
- [ ] Wait for CodeRabbit completion before merge.
- [ ] Address findings or record explicit accepted exceptions from user.
- [ ] Rerun required gates after fixes.
- [ ] Write `post-review` checkpoint with findings and revalidation evidence.
- [ ] Merge only after review completion and green validation.
- [ ] Write `post-merge` checkpoint with merge commit and merged PR URL.

## Minimum Demo Scenarios

- [ ] Create a goal, project, and task in `Strategy`.
- [ ] Link a task to an existing board card.
- [ ] Attempt duplicate link and verify `409`.
- [ ] Force-reassign a link and verify old link clears atomically.
- [ ] Link a task to an existing job.
- [ ] Verify `latest_run_id` updates from linked runtime activity without changing task `updated_at`.
- [ ] Mark a task blocked and require `blocked_reason`.
- [ ] Move blocked task to done and verify reason clears.
- [ ] Verify stale work appears after threshold.
- [ ] Verify goal progress derives from non-archived leaf tasks only.
- [ ] Verify unattributed spend appears when links are missing.
- [ ] Import and export bootstrap preset with no secrets.
- [ ] Disable `strategy_hub` and verify existing tabs and saved cockpit layouts stay stable.

## Completion Criteria

- [ ] All checklist batches are complete.
- [ ] Folded spec and implementation still match.
- [ ] Checkpoint chain is complete and current.
- [ ] PR review workflow is fully satisfied.
