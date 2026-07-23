# Glass Office: Claude Resume Handoff

This is the single resume document for Claude after backend/ExecAss PR #98 and
Glass Office P0-P3 PR #99.

## Resume point

- Workspace: `Z:\carsinos-clean`
- Branch: `codex/glass-office-p4-p5-resume`
- Base/merged head: `cfd87e04bc7113c1818b0191abd11a0287fdc91e`
- Checkpoint track: `GLASS_OFFICE_P3_INTEGRATION WORK`
- Repository state at handoff creation: merged `main` plus this document only
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
- The Window: authoritative Reef presence plus safe Office Chatter.
- Agent Mail remains the canonical communication store. A bounded backend
  projector creates allowlisted safe work notes from ExecAss lifecycle events.
  `GET /office/chatter` is read-only.
- Protected theme values (`claw`, `claw-soft`, `ok`, `warn`) cannot be replaced
  through custom or persisted themes.
- Exact-head CI, CodeRabbit, frontend tests, browser tests, and Rust security
  gates passed before PR #99 merged.

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
| Send authenticated owner note | `POST /api/v1/office/chatter/{thread_id}/messages` |

Use the ExecAss integration handoff for all 16 versioned ExecAss operations and
the websocket resume/refetch rules. Never guess request proofs or cursor fields.

## What Claude still owns

### 1. Close the deferred foundation UX

- Theme editor MVP: edit allowed tokens, live preview, save/duplicate,
  export/import JSON. Protected tokens remain immutable.
- Office arrange mode: reorder, resize, hide/show, and add from the block
  registry.
- “Pin to Office” must add a registered block/config entry, not copy data or
  create a second source of truth.
- Finish the Assistant’s Desk as the deliberate slide-over conversation
  destination described in the design, without restoring the retired legacy
  Assistant Desk data path.

### 2. Finish the experiential layer of P3

- Reef crab/report-card interaction and authoritative deep links for
  delegation/session/run targets.
- Reduced-motion behavior and polished honest `unknown`/empty/error states.
- Chatter room ergonomics, quiet unread treatment, and responsive expansion.
- Do not invent typing/threading capabilities that Agent Mail does not expose.

### 3. P4 — The Trenches

Rehome existing capabilities without losing parity:

- Boards
- Calendar
- Plan/Strategy
- Staff Directory
- History & Receipts / Runbook

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

After reading the references and writing the phase-start checkpoint, begin with
the deferred foundation UX (theme editor + arrange/pin mechanics). Those are the
shared primitives P4 and P5 should consume; do not start by hardcoding Trenches
or Basement navigation.
