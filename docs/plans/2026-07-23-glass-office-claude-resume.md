# Glass Office: Claude Resume Handoff

This is the single resume document for Claude after backend/ExecAss PR #98,
Glass Office foundations PR #99, foundation UX PR #100, Assistant's Desk
PR #101, P3 experiential PR #102, the first P4 Trenches room slice PR #103,
and the Calendar room slice in PR #104.

## Resume point

- Workspace: `Z:\carsinos-clean`
- Branch: `codex/glass-office-p4-strategy`
- Base/merged head: `c63e60e604c594d39f2ce4051f7af8f10b6cfd98`
- Checkpoint track: `GLASS_OFFICE_P4_TRENCHES WORK`
- Repository state at handoff refresh: PR #104 is merged into `main`; the
  Plan/Strategy continuation branch was created from the exact merge commit.
  The pre-existing root `node_modules/` remains untracked and must not be staged.
- Old `Z:\carsinos` tree: DEV/history only; do not implement there

Start with:

```powershell
cd Z:\carsinos-clean
git status --short --branch
git rev-parse HEAD
Get-Content -Raw runtime/checkpoints/LATEST.md
Get-Content -Raw runtime/checkpoints/LATEST.json
```

Before UI work, follow `CLAUDE.md`: use the `frontend-design` skill and maintain
the checkpoint ledgers at phase start and after green validation.

## What is already real and merged

Do not rebuild or replace these:

- ExecAss v1.1 backend/runtime contract, one user + one ExecAss + one CarsinOS.
- The Office: briefing, ask-box intake, Needs You decisions, one-confirmation
  continuation, In Motion, Done/receipts, Next, tray, and freeze/resume.
- Native owner-proof signing and durable ExecAss cursor/resume behavior.
- Registry-driven elevator data, capability filtering, room identity, floor
  overrides, keyboard floor shortcuts, theme tokens, config persistence, block
  normalization, and density.
- Stable room IDs own navigation end to end. Shared-route rooms light exactly
  one lamp; direct clicks and floor shortcuts use the same capability,
  floor-override, and available-tab-filtered registry.
- The Window: authoritative Reef presence plus safe Office Chatter.
- P3 experiential Window: focus-correct crab report cards, honest freshness and
  unknown states, exact session/run targets with disabled-room refusal,
  reduced-motion scuttling, quiet unread treatment, grouped chatter, and
  responsive Reef disclosure.
- Agent Mail remains the canonical communication store. A bounded backend
  projector creates allowlisted safe work notes from ExecAss lifecycle events.
  `GET /office/chatter` is read-only.
- Protected theme values (`claw`, `claw-soft`, `ok`, `warn`) cannot be replaced
  through custom or persisted themes.
- Theme Studio, Office arrange/size/hide/show, and registry-backed Pin to Office.
- Boards is the first parity-proven P4 room. Its original board
  toolbar/controller/mutations remain intact; Pin to Office reveals a real
  registry shortcut block that walks back to the Boards room without copying
  board data. External pin/config changes update the mounted Office canvas.
- Calendar is the second parity-proven P4 room, merged in PR #104. Week View,
  Schedule, Active Jobs, heartbeat setup, job controls, Strategy context, and
  Runbook links remain on the original Calendar surface. Its registry shortcut
  returns by stable room ID and visibly refuses if Trenches is later disabled.
- The Assistant's Desk slide-over: persona/decision entry, sitting-only
  conversation, real signed intake attachment, live decision lookup, real
  revise resolution, delegation detail over the shoulder, vanished-decision
  handling, keyboard-modal isolation, and focus restoration.
- Exact-head CI, CodeRabbit, frontend tests, browser tests, and Rust security
  gates passed before PR #103 merged.

## Authoritative references

Read these in this order:

1. `docs/plans/2026-07-22-glass-office-design.md` — product experience and P4/P5.
2. `docs/plans/2026-07-22-glass-office-claude-brief.md` — locked product decisions.
3. `docs/EXECASS_FRONTEND_INTEGRATION_HANDOFF.md` — complete v1.1 API/WS matrix.
4. `docs/plans/2026-07-22-glass-office-backend-asks.md` — additive seams and fallbacks.
5. `CHECKLIST.md` and `runtime/checkpoints/LATEST.*` — execution/evidence truth.

Primary connecting points:

- Elevator/rooms: `apps/mission-control/src/glass/floors.ts`
- Themes: `apps/mission-control/src/glass/themes.ts`
- Persisted Glass config: `apps/mission-control/src/glass/config.ts`
- Office blocks/layout: `apps/mission-control/src/glass/blocks.ts`
- App shell: `apps/mission-control/src/app/AppShell.tsx`
- ExecAss client/types/stream:
  `apps/mission-control/src/glass/execass/`
- Window client/types:
  `apps/mission-control/src/glass/window/`
- Office controller:
  `apps/mission-control/src/features/execassOffice/`
- Window controller/view:
  `apps/mission-control/src/features/glassWindow/`
- Canonical generated contract: `contracts/execass/v1/`
- Gateway Office routes: `crates/carsinos-gateway/src/main.rs`
- Canonical Agent Mail/presence storage: `crates/carsinos-storage/src/lib.rs`

Live additive Office routes:

| Purpose | Method and path |
|---|---|
| Coarse authoritative presence | `GET /api/v1/office/floor-presence` |
| Read safe chatter rooms/messages | `GET /api/v1/office/chatter` |
| Send authenticated owner note | `POST /api/v1/office/chatter/rooms/{thread_id}/messages` |

Use the ExecAss integration handoff for all 16 versioned ExecAss operations and
the websocket resume/refetch rules. Never guess request proofs or cursor fields.

## What Claude still owns

### 1. Deferred foundation UX is complete

Completed in PR #100:

- Theme Studio edits allowed tokens with live preview, save/duplicate, and
  export/import JSON while protected tokens remain immutable.
- Office arrange mode reorders, resizes, hides/shows, and restores registered
  blocks within the fixed desktop canvas.
- “Pin to Office” adds or reactivates a registered block/config entry without
  copying data or creating a second source of truth.

Completed in PR #101:

- Assistant's Desk is the deliberate slide-over conversation destination.
- It uses the real Office controller/contracts and does not restore the retired
  legacy Assistant Desk data path.
- Conversation copy remains honest: the local log lasts only for this sitting;
  durable work becomes a delegation with receipts.

Do not rebuild these foundation pieces. Extend and consume them.

### 2. P3 experiential is complete

Completed in PR #102. Do not rebuild it. Preserve the exact-target selection
rule: an active Runbook filter may not replace an explicit Reef target with an
unrelated first result.

### 3. P4 — The Trenches

Rehome existing capabilities without losing parity:

- Boards — complete in PR #103
- Calendar — complete in PR #104
- Plan/Strategy
- Staff Directory
- History & Receipts / Runbook

PR #103 also completed the shared P4 room path: stable room-ID selection,
shared-route lamp ownership, resolved-registry keyboard jumps, live external
pin synchronization, honest pin failure/full-canvas states, and narrow-width
room marks. Extend that path; do not create another navigation mechanism.

The next bounded slice is Plan/Strategy. Preserve its
existing Overview, Goals & Projects, Tasks, Task Detail, and Insights surfaces;
draft-discard guards; goal/project/task mutations; summary lenses; Board and
Calendar links; and Runbook context.

Important boundaries:

- Current persistent agents may be shown as staff.
- Do not infer temporary task workers from names or `reports_to`.
- Omit task-worker nesting until an authoritative worker-lineage contract exists.
- Board moves remain the existing board mutation. Do not also POST ExecAss
  intake and do not claim “ExecAss heard it” without a server acknowledgment.
- No payroll, salary, payment, spending, tenant, or financial-role machinery.

### 4. P5 — The Basement

Rehome the existing operational surfaces into registry rooms:

- Connectors
- Models & Providers
- Breakers & Scheduler
- Event stream
- Directory / Front Desk
- Memory plant
- File-lock machinery
- Setup, gateway connection, feature toggles, and onboarding
- Plain-language ExecAss policy settings

Keep existing functionality reachable throughout the transition. A room may be
temporarily mapped to an existing tab, but stable room IDs—not display labels or
array positions—must own navigation/config identity.

### 5. Cross-floor completion

- Incident posture from existing authoritative failure/runtime/breaker facts.
  Ordinary waits and an owner-requested stop are not “fire alarms.”
- Guided tour and one-line floor guides updated for the elevator.
- Desktop fixed-canvas law; mobile may stack and scroll.
- Keyboard, focus, screen-reader, reduced-motion, empty/error/loading, and
  narrow-width QA.

## Product rails that must not drift

- This is not a multi-user/tenant product.
- CarsinOS confirms destructive or dangerous action once with the concrete
  consequence, then carries the confirmed reasoning forward. It does not police
  ordinary owner requests.
- Office Chatter is a view over Agent Mail, not COLACK and not a second store.
  `Z:\COLACK` is only an optional interaction/code-pattern donor.
- Do not add payroll or financial product machinery.
- Do not fabricate backend capability. Show honest unavailable/omitted states.
- Do not make P4/P5 shell branches hardcoded; extend registries/config.
- Preserve every existing Mission Control capability while changing where it
  lives.

## Required proof before handing back

At each phase boundary:

```powershell
cd Z:\carsinos-clean\apps\mission-control
npm run typecheck
npm run lint
npm run test:unit -- --run
npm run build
```

Also run focused Playwright coverage for every changed floor at desktop and
390px, with console-error and horizontal-overflow assertions. If a shared shell,
theme, provider, navigation, or layout file changes, visually verify every
affected surface before commit/push.

Before PR:

- Run the generated ExecAss contract check and independent validator.
- Run `git diff --check`.
- Audit every changed file for direct scope necessity.
- Use normal incremental CodeRabbit review; never spam manual re-review commands.
- Target `main` and include exact validation commands/results.

## First implementation command

After reading the references and writing the phase-start checkpoint, implement
Plan/Strategy as the third parity-proven P4 room slice:

1. Re-run the Calendar regression anchors from PR #104: all three surfaces,
   existing job/heartbeat/integration coverage, stable Calendar lamp, pin and
   repeat-pin truth, disabled-Trenches shortcut refusal, exact return, reload,
   desktop, and 390px.
2. Lock parity tests around Strategy's Overview, Goals & Projects, Tasks, Task
   Detail, and Insights surfaces plus its draft guards, mutations, summary
   lenses, and existing linked-work destinations before presentation changes.
3. Register a hidden-by-default `plan` room-shortcut Office block using the
   existing `plan` room ID and the shared resolved `onRoomSelect` path.
4. Add `PinRoomToOffice roomId="plan"` to the existing Strategy surface. Do
   not copy Strategy data, duplicate a mutation, or create another navigation
   mechanism.
5. Prove pin success/repeat/failure, live mounted-Office synchronization,
   disabled-floor refusal, exact room return, stable lamp identity, desktop,
   390px, console cleanliness, and no horizontal overflow.

Hand each bounded P4 slice back for hostile source and desktop/390 visual QA.
Theme Studio, arrange mode, registered/config-backed pinning, Assistant's Desk,
and P3 Window behavior are complete shared primitives.

## PR #101/#102/#103 QA lessons to carry forward

- A local green claim is not proof until a fresh lint/CI run reproduces it.
- Never display mutation success or clear owner input before the authoritative
  request succeeds.
- Use synchronous in-flight protection where same-tick duplicate submissions
  are possible; React render state alone is not a lock.
- Do not invent presence or runtime facts in friendly copy.
- Portaled slide-overs must trap focus, inert the background, close by Escape,
  restore the invoking control, and visually cover test/product overlays.
- Browser screenshots are required because DOM tests cannot detect transformed
  ancestor clipping, z-index collisions, or narrow-width presentation defects.
- Keep diffs reviewable; do not bury a small fixture addition in line-ending
  churn.
- A test that only proves a destination tab opened is not proof of an exact
  deep link; assert the requested entity/detail.
- A test that only finds tab labels is not proof that the surfaces are
  reachable; enter every non-default surface and assert its content.
- A persisted shortcut may outlive a floor override. Resolved room selection
  must return rejection to its caller, and the small Office block must show the
  refusal inside its visible bounds.
- Preserve owner text typed while an earlier async send is in flight, and use a
  synchronous lock for same-tick duplicate sends.
- Visually inspect both light and dark scoped themes; inherited global text can
  become unreadable when a local surface token changes.
- Navigation acceptance must use the exact resolved elevator registry rendered
  to the user. Validating against raw default floors can make hidden or
  capability-disabled rooms navigable.
- Config-backed mutations must notify live consumers, and mounted consumers
  must subscribe when the UI promises immediate feedback.
- When responsive CSS hides a room label, preserve a tested visible room mark;
  an active pill with no readable identity is not a mobile navigation state.
