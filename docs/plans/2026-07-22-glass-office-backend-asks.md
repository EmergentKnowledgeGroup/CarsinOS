# Glass Office — Backend Asks for Dex

Status: handoff from Claude (frontend) via the owner, 2026-07-22.
Context: the approved frontend redesign is specified in `docs/plans/2026-07-22-glass-office-design.md`. It is built entirely on the checked ExecAss v1.1 contract (`contracts/execass/v1/`) plus existing gateway APIs. This document lists the **only** places where the new experience wants backend support that I could not find in the contract or existing endpoints. Everything else is covered — decisions, proofs, run control, policy, receipts, events, summary projection all map cleanly. Nice work.

Ground rules I am holding on my side (so you don't have to wonder):

- No second product truth, no frontend lifecycle/approval/danger logic, no dual reads, no proof material in React/browser storage — per your handoff, unchanged.
- Every ask below is *additive*. If an ask conflicts with the locked v1.1 controls, the locked controls win and I'll design around whatever you can give me.
- "P0/P1/P2" = how much of the new UI blocks on it (P0 blocks a core floor; P1 degrades a floor; P2 is future/optional).

---

## Ask 1 — Agent working-chatter substrate ("Office Chatter" / COLACK viewport) — P1

**Product intent:** Floor 3 shows the org's working chatter as a live, Slack-shaped channel viewport (channels ≈ workstreams; agents post short action/coordination notes as they work; ExecAss replies; the owner may speak but never has to). It is ambient observation — anything needing the owner still arrives only via typed attention on the Office floor.

**What the frontend needs:** a channel-shaped, subscribable stream: channel list, messages (sender identity, timestamp, text, optional thread ref/attachment ref), unread state, and a send path for the owner. Read-heavy; presentation is mine.

**Open questions for you:**
1. Substrate choice: present existing Agent Mail rooms as this stream? A dedicated room-per-workstream convention ExecAss maintains? Or a real COLACK wire-up (`Z:\COLACK` — Rust, Postgres, webhooks) via connector? Owner is enthusiastic about COLACK branding either way; the wire substrate is your call.
2. Do agents currently emit any "working note" traffic naturally (tool-call summaries, run milestones) that could seed channels, or does ExecAss need to author chatter deliberately?
3. Redaction: chatter must be safe-payload class (no secrets/raw tool output). Existing Agent Mail redaction sufficient?

**Fallback if deferred:** Floor 3 ships with Reef only; chatter panel shows a tasteful "not wired yet" state. No dual truth invented.

## Ask 2 — Floor presence projection (the Reef) — P1

**Product intent:** ambient aquarium of the crew: who is "out," a coarse current-task label, and a health mood (healthy / busy / recovering / idle / offline). Click → short report + deep link. Coarse presence only, never message content.

**What the frontend needs:** per-agent: id, display name, employment kind (see Ask 4), coarse activity label (human-safe string), mood enum, active delegation/session ref for deep link. Poll or WS — either fine; ~5s freshness is plenty.

**Question:** derivable today from sessions/runs/jobs + events without new backend work? If yes, tell me the blessed combination and I'll build a client-side projection. If that requires stitching 4 endpoints and inferring, a tiny `GET .../floor-presence` (or event family) would be cleaner and stays a projection, not a truth.

## Ask 3 — Boss board-moves ping ExecAss — P2

**Product intent:** in the Trenches, when the owner drags/moves a board card, ExecAss should "hear about it" as intent (owner moved X to column Y) rather than the org silently diverging from what ExecAss believes.

**Question:** do board mutations already emit events ExecAss ingests? If yes, nothing needed. If no: is the right shape an event ExecAss subscribes to, or should the frontend send a lightweight `POST /execass/intake` note (`attach_to_delegation_id` where known) alongside the board mutation? I can do the latter today if you bless the pattern (idempotent, owner-authored, no new endpoint).

## Ask 4 — Staff vs. task-worker distinction — P1

**Product intent:** Teams becomes "the payroll": permanent staff only (ExecAss + wired-in model staff, e.g. Claude/Vale/Lyra). Ephemeral task-workers render nested *inside* their employer's card, and per-staff memory drill-in lives there too ("what does Claude remember?").

**What the frontend needs on agent records:** (a) an employment kind (permanent staff vs. task-worker), (b) parent/employer agent ref for workers. Today I can only heuristic this from `reports_to` + naming, which will misfile someone eventually.

## Ask 5 — Briefing prose (ExecAss's morning voice) — P2

**Product intent:** the Office opens with a short assistant-written brief ("Morning, boss. Quiet night — two things for you…") derived from the summary projection. Structured data is fully covered by `GET /summary`; the question is only the prose.

**Options:** (a) frontend composes deterministic template prose from the projection — zero backend work, ships first; (b) backend "briefing composer" that writes it in ExecAss's voice (LLM), returned alongside or via summary, with the projection remaining the sole truth underneath. I'll ship (a) regardless; flagging (b) as the eventual nicer voice. No fabricated activity either way — brief must only restate projection facts.

## Ask 6 — Incident posture signal — P2 (likely already covered — please confirm)

**Product intent:** the calm office changes posture when something is genuinely wrong (fire alarm: banner + walk-me-there). I plan to derive posture client-side from existing signals: breaker states, connector health, `execass.v1.receipt.integrity_failed`, `runtime_host.changed` faulted states, and recovery attention items. **Confirm** that set is sufficient and there's no better "health posture" source I'm missing. No new severity taxonomy wanted.

## Ask 7 — Later / concepts (no action now)

- **"Call the Tech":** an MCP jack-in point in the Basement where a model (Dex/Claude/other) can inspect machinery on request and answer in plain words. Owner loves it; concept-stage; backend/MCP story is yours whenever.
- **Server-side sync of themes/layouts:** frontend persists locally (Tauri store) for now; a small user-prefs blob endpoint would enable multi-device later.
- **Notification history:** "While you were out" tray currently builds on client-observed events + `notification.scheduled` + summary ack revisions. If a queryable notification history exists/lands someday, I'll consume it.

---

## Explicit non-asks

No changes to: decision/confirmation semantics, proofs, run control, policy authority path, summary projection shape, WS resume protocol, error catalog. No spending/finance/tenant/role concepts. No second approval engine, scheduler, or receipt authority. The v1.1 contract as shipped is the foundation and it is sufficient for Floors 4, 2, and B in their entirety.

**Parallelization note for the owner:** nothing here blocks frontend start. P0–P2 of the design doc (foundations, plumbing, the Office) run entirely on the existing contract. Asks 1–2 gate only Floor 3's live data; Ask 4 gates only payroll truthfulness (heuristic fallback exists).
