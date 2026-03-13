# carsinOS Visual Runbook Execution Checklist

Date: 2026-03-09  
Source spec: [CARSINOS_VISUAL_RUNBOOK_SPEC.md](./CARSINOS_VISUAL_RUNBOOK_SPEC.md)

## Purpose

Execution checklist for building the visual runbook end-to-end from the finalized spec.

Use this checklist to sequence the build, force the canonical runbook model to land before UI flourishes, and keep validation, checkpoints, and PR flow aligned with repo SOP.

## Operating Rules

1. The runbook is a derived truth layer, not a second execution engine.
2. Do not add a workflow editor, office view, or decorative animation in Version 1.
3. `Runbook` must land additively and must not break current Mission Control flow.
4. All configurable limits, thresholds, and refresh intervals must live in dedicated config, not inline constants.
5. Write or update tests so they can fail on false-positive success, stale-state drift, missing links, and invented progress.
6. Use the `frontend-design` skill for all UI/UX implementation work on the runbook surface and cross-surface UI changes.
7. Update `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json` at:
   - implementation phase start
   - post-green validation for each implementation phase that changes UI
   - PR open
   - post-merge
8. Before each major edit block and after each meaningful milestone, run the context checkpoint snapshot command.
9. Do not merge before `@coderabbitai review` completes and findings are handled.

## Entry Criteria

- [ ] Final spec is accepted as source of truth.
- [ ] Working branch and head are verified.
- [ ] Existing dirty changes outside scope are identified and left untouched.
- [ ] Required local gate stack is known:
  - `npm run lint`
  - `npm run typecheck`
  - `npm run test:unit`
  - `npm run build`
  - `npm run test:e2e:core`
  - `python3 scripts/mission_control_quality_gate.py --profile pr`
  - `python3 scripts/mission_control_quality_gate.py --profile release --fail-on-blocked`
  - `scripts/security_pr_gate.sh`

## Batch 0: Implementation Prep

### Goal

Freeze the implementation starting point and verify the shell seams before code changes.

### Checklist

- [ ] Write implementation phase-start checkpoint.
- [ ] Re-read the finalized spec and this checklist before code changes.
- [ ] Confirm current tab order, keyboard shortcuts, and optional-module gating seams.
- [ ] Confirm existing websocket refresh and Mission Control bootstrap seams.
- [ ] Confirm current mock gateway and e2e harness seams for new runbook coverage.

### Exit Criteria

- [ ] Runbook implementation can start without ambiguity about shell, config, or test harness seams.

## Batch 1: Contracts, Templates, and Backend Read Model

### Goal

Land the canonical runbook model and backend truth rules before any UI work depends on them.

### Checklist

- [ ] Add protocol types for runbook list/detail/summary, step state, actions, source facts, warnings, and availability.
- [ ] Add or factor a code-defined runbook template catalog for all four Version 1 kinds.
- [ ] Encode deterministic selection rules:
  - selected execution resolution
  - root status precedence
  - active-step fallback
  - next-step ordering
  - blocked-vs-waiting mapping
- [ ] Reuse existing persisted truth for runs, job runs, approvals, task links, board-card links, and sessions.
- [ ] Add gateway read-model builders for runbook list/detail.
- [ ] Add list endpoint with stable ordering, opaque cursor behavior, filter support, and config-defined limits.
- [ ] Add detail endpoint with `source_facts`, `availability`, `warnings`, `owner_agent_id`, and `owner_agent_label`.
- [ ] Add optional summary endpoint only if it avoids duplicating derivation logic.
- [ ] Gate new endpoints cleanly behind `runbook_hub` behavior or ensure frontend never calls them when disabled.
- [ ] Add explicit `anchor_missing`, `invalid_cursor`, `inconsistent_runbook_links`, and `feature_disabled` behavior.
- [ ] Add or update backend tests for:
  - each supported runbook kind
  - anchor-only snapshots
  - mixed terminal + stale non-terminal facts
  - blocked-vs-waiting mapping
  - limited-state rendering rules
  - cursor stability and invalidation
  - feature-disabled endpoint behavior

### Exit Criteria

- [ ] Backend contracts and read models are decision-complete and tested.
- [ ] No UI layer needs to invent state or guess transitions.

## Batch 2: Frontend Shell, Config, and Runbook Surface

### Goal

Introduce `Runbook` as a gated top-level Mission Control surface without disturbing the current UX.

### Checklist

- [ ] Add `runbook` to Mission Control tab types and tab registry.
- [ ] Append `Runbook` after `Strategy`.
- [ ] Add the new shortcut without regressing existing `1-0` behavior.
- [ ] Add `runbook_hub` to runtime ops UX config, defaults, sanitization, and tests.
- [ ] Add a dedicated runbook config module for page size, history limit, refresh debounce, stale threshold, and summary limits.
- [ ] Build the runbook controller around typed list/detail reads plus websocket-driven refresh.
- [ ] Add stale-state indication, reconnect refresh, focus refresh, and manual retry behavior.
- [ ] Build the Runbook landing surface with:
  - summary strip
  - filter bar
  - canonical runbook list
  - detail pane
  - plain flow renderer
  - history timeline
  - linked artifacts/actions
- [ ] Keep the renderer semantic, keyboard-accessible, and non-theatrical.
- [ ] Add loading, empty, error, limited, and feature-disabled states.
- [ ] Update help/tour text if Runbook should appear in product guidance.
- [ ] Add frontend tests for controller behavior, stale indicators, filters, detail selection, and rendering of `limited`/warning states.
- [ ] Add new e2e coverage for the Runbook tab itself.

### Exit Criteria

- [ ] Operator can open the Runbook tab and navigate trustworthy runbook list/detail views.
- [ ] UI behavior remains additive and stable when `runbook_hub` is off.

## Batch 3: Cross-Surface Integrations

### Goal

Expose runbook entry points and context chips where they help, without turning other tabs into secondary runbook renderers.

### Checklist

- [ ] Add `Open in Runbook` and status/context chips to Strategy task detail/context.
- [ ] Add `Open in Runbook` and status/context chips to Boards card detail/context when linkable.
- [ ] Add `Open in Runbook` and status/context chips to Calendar job rows/detail.
- [ ] Add `Open in Runbook` actions to Focus items for approvals and failures.
- [ ] Add `Open in Runbook` actions to Assistant when a current/latest run exists.
- [ ] Add cockpit summary widgets or data-source integration using shared runbook derivation only.
- [ ] Ensure hidden runbook widgets and entry points degrade cleanly when `runbook_hub=false`.
- [ ] Add/update tests for in-app deep-link behavior across Strategy, Boards, Calendar, Focus, Assistant, and Cockpit.

### Exit Criteria

- [ ] Other tabs gain runbook entry points and status context without role confusion or behavior regressions.

## Batch 4: Integration Validation

### Goal

Prove the end-to-end implementation against the spec before PR creation.

### Checklist

- [ ] Run targeted backend tests for protocol/gateway/storage changes.
- [ ] Run `npm run lint`.
- [ ] Run `npm run typecheck`.
- [ ] Run `npm run test:unit`.
- [ ] Run `npm run build`.
- [ ] Run `npm run test:e2e:core`.
- [ ] Run `python3 scripts/mission_control_quality_gate.py --profile pr`.
- [ ] Run `python3 scripts/mission_control_quality_gate.py --profile release --fail-on-blocked`.
- [ ] Run `scripts/security_pr_gate.sh`.
- [ ] Write post-green checkpoint with command evidence and residual notes.

### Exit Criteria

- [ ] Local validation stack is green or any blocker is explicitly documented.

## Batch 5: PR and Review Flow

### Goal

Complete the required PR flow after local gates are green.

### Checklist

- [ ] Open PR.
- [ ] Write `PR open` checkpoint with PR URL and CodeRabbit request evidence.
- [ ] Request `@coderabbitai review`.
- [ ] Wait for CodeRabbit completion before merge.
- [ ] Address findings or record user-accepted exceptions.
- [ ] Rerun required gates after fixes.
- [ ] Write `post-review` checkpoint with findings and revalidation evidence.
- [ ] Merge only after review completion and green validation.
- [ ] Write `post-merge` checkpoint with merge commit and merged PR URL.

## Minimum Demo Scenarios

- [ ] Open Runbook from the new top-level tab and inspect list/detail for all supported states.
- [ ] Assistant run -> approval wait -> resolve -> resume -> Runbook updates truthfully.
- [ ] Board card with linked run opens Runbook from Boards and shows the correct selected execution.
- [ ] Job failure opens from Focus into Runbook and preserves warning/next-step truth.
- [ ] Strategy task with linked execution opens Runbook and shows task-driven blocked/waiting semantics.
- [ ] `runbook_hub=false` hides Runbook safely and avoids broken links or hidden background refresh.
- [ ] Deleted primary anchor returns `anchor_missing` and is omitted from list.
- [ ] Inconsistent secondary links render `limited` with warnings when allowed.

## Definition of Done

- [ ] Backend runbook contracts and read models are implemented and tested.
- [ ] Mission Control exposes a plain, trustworthy Runbook tab behind `runbook_hub`.
- [ ] Cross-surface runbook entry points are additive and non-disruptive.
- [ ] Local regression and repo gate stack are green.
- [ ] PR workflow is completed with CodeRabbit review before merge.
