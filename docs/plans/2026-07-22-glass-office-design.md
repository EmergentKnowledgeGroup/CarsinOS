# The Glass Office — CarsinOS Frontend Redesign (Design Document)

Status: validated with the owner on 2026-07-22 through interactive brainstorm + clickable sketch iterations (v1–v4).
Owner sign-off: direction approved ("I actually feel SUPER STRONG about this direction").
Author/implementer: Claude (frontend only).
Companion document: `docs/plans/2026-07-22-glass-office-backend-asks.md` (handoff for Dex — backend asks).
Prior authority: this design implements, and must not contradict, `docs/EXECASS_FRONTEND_INTEGRATION_HANDOFF.md`, `docs/EXECASS_FRONTEND_EXPERIENCE_BRIEF.md`, `docs/EXECASS_BACKEND_PRODUCT_BEHAVIOR_BRIEF.md`, and the checked v1.1 contract in `contracts/execass/v1/`.

---

## 1. Vision

> Mission control in capability. Executive assistant in experience.

The metaphor that survived every design test: **the boss's glass office at the top of the tower.**

The old Mission Control seated the user *on* the production floor with every machine beeping at equal volume. The Glass Office moves the user *upstairs*: calm, quiet, elevated — looking down through glass at an organization that is visibly alive and working without them. ExecAss runs the floor. The boss delegates outcomes, answers the few decisions that genuinely need a human, and leaves.

**The final design test for every screen:** does this help the user work *with ExecAss*, or does it ask the user to *operate CarsinOS*? If it teaches machinery, redesign it.

### The escalation ladder (interaction model)

Every interaction lives on one of four rungs. Never skip a rung upward without the user asking.

1. **Glance** — ambient truth (the Window/Reef, quiet-notes). Asks nothing.
2. **Ding** — things come *to* the boss (codec pop-up / Needs You card). Resolvable in 1–2 clicks. States consequence in plain words. Work visibly continues after resolution; never a second nudge.
3. **Walk over** — a decision that needs conversation escalates the boss *to* the Assistant's Desk with full context ("let's talk it through").
4. **Open the drawer** — raw truth (receipts, transcripts, events, IDs, logs) is always exactly one deliberate click from any claim, never ambient.

### Design laws (non-negotiable)

- **The Office is a room, not a document.** No page scroll on the primary desktop surfaces. Fixed-viewport canvas. Blocks trade space; the canvas never grows. (Chat/feed viewports may scroll internally — a chat log is a window, not a layout failure.)
- **Density-adaptive blocks.** A block shrunk to small shows a summary (one number + one whisper); medium shows top items; large shows everything. Content adapts to size; nothing becomes a tall skinny column.
- **Config-driven everything.** Layouts, blocks, and themes are data, not code (see §5). What is shown, its size, and its order is the user's choice.
- **Come-to-the-boss.** Attention travels to the user; the user never hunts for what needs them.
- **Truth at the right depth.** Never solve complexity by hiding truth; solve it by layering it. Proof one click away, raw payloads only on explicit request.
- **Calm by default, honest under failure.** When nothing needs the user, the interface is reassuringly quiet. When something is wrong, it changes posture (see Incident posture, §4.6) rather than hiding it.
- **No permission theater.** Ordinary owner instructions proceed. One confirmation for a dangerous action, stating the consequence; a confirmed unchanged action continues forever (contract-guaranteed).

---

## 2. The floor plan

Navigation is an **elevator** (left rail, glowing floor lamps). Four floors. Keyboard: `4/3/2/B` (or 1–4 positional), `Cmd/Ctrl+K` command palette ("the intercom") from anywhere.

### 4F — The Office (default / home)
The boss's room. Morning briefing in ExecAss's voice (serif, written-to-you prose, clamped short with "read the full brief"). The **ask-box** ("What do you need?") for delegating outcomes in plain language — one intake, no subsystem choice (POST /execass/intake). Below, the **block canvas** (config-driven grid): Needs You, In Motion, Done Since You Checked, Next, Office Chatter peek, Pinned-from-the-Trenches shortcuts, "+ add a block" ghost slot.

- **Needs You** renders ExecAss `AttentionItem`s only (`waiting_for_user`). Decision cards show: kind, plain-language title, why-now, consequence (highlighted for dangerous confirmations), recommendation, alternatives as buttons ("Yep — do it" / "Let's talk it through" / decline / see the proof). Resolution carries the exact revision/challenge and native decision proof.
- **"While you were out"** — a quiet notes tray (notification history / missed dings). Deliberately NOT part of Needs You and NOT a red badge; it is notes ExecAss left on the desk. Calm by design so the daily open never spikes cortisol.
- **The Assistant's Desk** — part of Floor 4, not a separate floor. A slide-over destination ("walking over"): full conversation with ExecAss, the current plan over-the-shoulder, linked docs/artifacts, pinned session transcripts. Reached by: "Let's talk it through" on any decision, clicking the ExecAss persona, or the intercom. Descends from the old Assistant chat page. This is a destination, not where the boss lives.
- **The freeze switch** — global stop/drain ("everybody freeze"): visible, dignified, near the ExecAss status. Uses contract stop-all/resume-all with run-control proof; per-delegation stop/resume appears on delegation detail. Stop state is orthogonal to phase and the UI must render it that way.

### 3F — The Window
The aquarium. **Reef** (left): ambient canvas of the crew — crabs scuttling about their work; touch a crab, it reports (coarse state + deep link). Truthful, privacy-preserving, zero obligations — the screen you leave on while playing Palworld. **Office Chatter** (right): a COLACK-style channel viewport (channel sidebar, live messages, typing indicators, threads) showing agents' working chatter as it happens. Overheard, not assigned: anything needing the *user* still arrives as a ding on 4F. (Data substrate = Dex conversation; see backend-asks doc.)

### 2F — The Trenches
Direct involvement, on purpose — "where the boss gets out of the chair." Sub-rooms (tabs): **Boards** (full kanban parity with today: card editor, scripts, assets, run-card; boss drag/moves emit intent to ExecAss — nothing shuffles silently), **Calendar** (month/week, run-now/pause/resume, every date clickable), **Plan** (Strategy: goals/projects/tasks CRUD, insights/spend), **Teams — the payroll** (permanent staff only: ExecAss + wired-in model staff; each staff card contains its own live subagent chips and an "open their memory" drill-in; org tree; Hire), **History & Receipts** (delegation history table, receipt chains, runbook explorer). Simple lock/reservation touch-points surface here; lock machinery lives in the Basement. Anything here can be **pinned to the Office** as a block.

### B — The Basement
The machinery / NOC. Never shouts upstairs. Rooms: **Connectors** (health, latency), **Models & Providers** (routing, budgets as technical quotas), **Breakers & Scheduler**, **Event stream** (full raw feed with filters — the old Events page), **Directory / Front Desk** (People & Routing: human identities, platform links, per-person assistant routing, DM policies), **Memory plant** (global memory health; per-agent memory lives on staff cards), **File locks** (guts), **Setup** (gateway connect, tokens, feature toggles, onboarding wizard), and **Call the Tech** (MCP jack-in console — concept, backend-gated). ExecAss policy amendments (PUT /policy with owner proof) surface here as plain-language settings.

---

## 3. Brand

- **CarsinOS = Carcinus (crab) + Orchestration System.** Rust. Carcinization. Crabs all the way down. The brand leans in, tastefully.
- **The mark:** the owner's claw logo — claw-orange crescent claw forming a C, crack detail, terminal `>` held in the pincer. Source: `Z:\carsinos-codex-work\Visuals\logo.png` (+ `brandcollage2.png`). Recreate as clean SVG for the app; wordmark "Carsin**OS**" with OS in Claw Orange; subtitle "crab orchestration system."
- **Claw Orange is constitutionally protected.** `#FF5A1F` (dark ground) / `#E8541B` (light ground). It is the *life* color: the mark, unread/presence dots, staff accents, reef crew. It is never themable away.
- **Default themes:** Light = porcelain glass office (warm whites, lagoon-teal operational accent, attention gold). Dark = "after hours" in Carbon/Void neutral blacks-and-greys (per brand collage: Carbon #1A1A1D, Void #0B0B0E, Steel #3A3A3F), not deep green.
- **Type:** editorial serif voice for ExecAss prose (the briefing reads like a written note), humanist sans for UI, mono for data/receipt IDs. Final faces chosen at implementation (bundled, no CDN).
- **Voice:** calm authority, plain words, honest about uncertainty, lightly crab-flavored where it delights ("the floor," "the crew"), never at the expense of clarity.
- Vale's collage tone ("COMMAND THE DRIVE") is car-flavored and not adopted; palette and mark are.

---

## 4. Experience specs (per surface, condensed)

4.1 **Briefing** — assistant-written prose summarizing: what changed, what finished (with receipt links), what needs the boss, what's next. Never invents activity; derived from the summary projection. Prose composition strategy TBD with Dex (template vs. composed) — see backend asks.

4.2 **Decisions (dings + cards)** — codec-style pop-up for arrivals (slide up, one-line answer or escalate, *BEEBOOP*, slide down; never a full-screen takeover). Cards persist in Needs You until resolved. Dangerous confirmations show the declared consequence verbatim. `revise` opens a text path; `decline`/`stop` are always present but never nag. After resolve: visible continuation ("work continues — no second nudge, ever"). 409/revision conflicts refetch-and-reconcile silently, re-present only if the decision materially changed. The window-close runtime confirmation stays a separate native flow (already wired) and is never merged with work decisions.

4.3 **In Motion / Done / Next** — from the summary projection only. `waiting_for_user` never appears in In Motion; `waiting_external` is visibly "on the world, not on you." Done shows outcomes in plain language with receipt links; honest partials rendered as partials (◐). Next shows routine/commitment/deadline/follow-up.

4.4 **Reef** — coarse presence only (never message content): who's out, current task label, health mood. Crabs scuttle; ExecAss is the distinct big crab. Click → report card → deep link to authoritative detail. Respects reduced-motion.

4.5 **Chatter/COLACK viewport** — channels ≈ workstreams; agents post action notes and coordination; ExecAss replies; boss may speak, never has to. Read-first; unreads are quiet dots, not badges screaming.

4.6 **Incident posture (the fire alarm)** — when the backend reports genuine trouble (breaker open, connector down, `receipt.integrity_failed`, runtime fault, recovery needing a human), the office changes posture: quiet-note flips state, an incident band appears with a plain-language sentence and one button that walks the boss to the exact room. Auto-recovers to calm. Severity taxonomy driven by existing events/breaker states; no invented severities.

4.7 **Help & first-run** — the floor plan itself is the primary onboarding (a caveman pressing `2` should understand the Trenches on sight). Retained: a light guided tour (elevator-stop by stop), per-floor one-line quick guides, docs reachable from the rail, and the connect wizard in Basement·Setup for first run.

4.8 **Mobile / small screens** — rail collapses to a top bar; Office becomes a stacked feed ordered by the information hierarchy (Needs You always first, plainly labeled); dings remain one-thumb resolvable; Reef/chatter summarize before expanding. Scroll is acceptable on mobile; the no-scroll law is a desktop law.

---

## 5. Config-driven architecture (owner requirement — core, not garnish)

**Themes are data.** A theme = a JSON token bag (`ground/surface/glass/ink×3/line×2/accent×3/gold×3/ok/warn/shadows/fonts/density`). Components consume CSS custom properties only — zero hardcoded colors in components. Ship Light + Dark (Carbon). A **theme editor** (color pickers over the token list, live preview, save/duplicate/export/import JSON) lets users build their own. Claw Orange + semantic safety colors (ok/warn/danger legibility) are non-themable constants. Density (comfortable/compact) is a token preset. Persistence: local (Tauri store) first; server sync optional later.

**Layouts are data.** The Office canvas = an ordered list of `{blockId, size(s/m/l), visible}` resolved against a **block registry** (each block: id, title, data source, s/m/l renderers, settings). Arrange mode = edit that data (drag, resize, hide, add-from-library). "Pin to Office" from any Trenches/Basement room = adding a registry block. The old Cockpit's widget-grid builder (export/import layouts, templates) is the in-house precedent and parts donor.

**Modules are data.** Floors/rooms declare themselves in a registry (id, floor, title, capability requirements) so features can be added/gated without shell surgery — successor to today's feature-toggle system.

---

## 6. Contract integration (the wiring underneath)

Everything renders from the v1.1 ExecAss contract; the frontend adds **no second truth**:

| Surface | Contract source |
| --- | --- |
| Needs You / dings | `SummaryResponse.needs_you` (`AttentionItem`), decisions via `DecisionSummary` |
| Decision resolve | `POST /decisions/{id}/resolve` + `LocalDecisionProof(Binding)` via Tauri `sign_execass_local_decision` |
| Ask-box | `POST /intake` (+ `X-ExecAss-Owner-Proof` via `sign_execass_local_owner_intake`) |
| In Motion / Done / Next / Receipts | summary projection; detail via `GET /delegations/*`, receipts endpoint |
| Freeze switch / stop-resume | stop-all & delegation run-control + `LocalRunControlProof` via `sign_execass_local_run_control` |
| Policy settings | `GET/PUT /policy` + owner mutation proof |
| Live updates | `/api/v1/ws` → `execass.v1.resume` cursor protocol; events = invalidation signals only; `summary_refetch_required` → discard-speculative, refetch, resume with returned `consumer_cursor` |
| Briefing ack | `POST /summary/ack` (displayed revision) |
| Errors | `safe_human_message` only; never raw payloads/internals |

Native proofs: requested from the Tauri shell per exact server-derived binding, submitted once, discarded. Never generated, persisted, or approximated in React/browser storage. All four Rust signing commands exist; the TypeScript wrappers in `src/lib/runtime.ts` are mine to add.

Legacy retirement per handoff: assistant-desk summary route, legacy approvals UI, and legacy Desk DTOs are cut over with **no dual-read fallback**; transcript path retained only until Desk/receipts replacement proves equivalent scope + redaction.

---

## 7. Implementation phases

Order marries the handoff's recommended cutover order (types → adapter → controller → summary cutover → decisions → the rest). Each phase: `frontend-design` skill engaged for UI work, checkpoint at phase start + post-green, `npm run typecheck && lint && test:unit && build` (+ `test:e2e:core` where E2E touched), PRs < 500 net LOC where feasible, desktop+narrow visual verification per repo rule.

- **P0 — Foundations:** token/theme engine (Light + Carbon Dark), theme editor MVP, block-registry + fixed-canvas grid with density adaptation, elevator shell, brand assets (SVG mark), keyboard map. Old tabs keep working during transition.
- **P1 — Contract plumbing:** generated/hand-mapped TS types from `contracts/execass/v1/schema/`, schema-shaped fixtures, `execassApi` adapter (all 17 operations, idempotency + x-request-id + 409 reconcile), Tauri proof wrappers, one ExecAss store (summary/detail authoritative, WS events as invalidation, durable cursor resume). Mock-gateway ExecAss fixtures + WS invalidation behavior.
- **P2 — The Office:** briefing band + ask-box (intake), block canvas with real projection data, Needs You cards + codec dings + decision resolution with proofs (one-confirmation continuation), While-You-Were-Out tray, freeze switch, summary-ack. **Primary summary consumer cuts over here; legacy desk consumer retired.**
- **P3 — The Window:** Reef canvas on real presence (Dex-dependent; graceful placeholder until wired), COLACK chatter viewport on the agreed substrate (Dex-dependent).
- **P4 — The Trenches:** remap boards/calendar/strategy/runbook/history into the floor with full capability parity; payroll Teams with nested subagents + per-staff memory drill-in; pin-to-Office.
- **P5 — The Basement:** NOC rooms, Directory/Front Desk, Setup + onboarding wizard rehome, policy surface, event stream, memory plant, locks guts.
- **P6 — Posture & polish:** incident posture, guided tour + quick guides + docs, mobile pass, reduced-motion/a11y sweep, E2E scenarios per handoff §7 (reconnect/refetch, idempotent retry, revision conflict, one confirmation + continuation, ordinary no-prompt work, external wait, recovery, partial completion, safe errors), performance.

P3 can slide after P4/P5 if Dex's chatter/presence work lands later; nothing else blocks on him.

---

## 8. Success tests (from the experience brief, kept literal)

Ten-second situational understanding on open; "does anything need me?" answerable instantly; delegation without choosing a subsystem; consequential decisions resolvable informed, in ≤2 clicks, with visible continuation; outcomes in plain language with proof one click away; comfortable on desktop and mobile; every current capability reachable without being permanently in the boss's face — and the caveman test: any floor understandable on sight, unga bunga included.
