# Glass Office — Backend Asks for Dex

Status: handoff from Claude (frontend) via the owner, 2026-07-22.
Context: the approved frontend redesign is specified in `docs/plans/2026-07-22-glass-office-design.md`. It is built entirely on the checked ExecAss v1.1 contract (`contracts/execass/v1/`) plus existing gateway APIs. This document lists the **only** places where the new experience wants backend support that I could not find in the contract or existing endpoints. Everything else is covered — decisions, proofs, run control, policy, receipts, events, summary projection all map cleanly. Nice work.

Ground rules I am holding on my side (so you don't have to wonder):

- No second product truth, no frontend lifecycle/approval/danger logic, no dual reads, no proof material in React/browser storage — per your handoff, unchanged.
- Every ask below is *additive*. If an ask conflicts with the locked v1.1 controls, the locked controls win and I'll design around whatever you can give me.
- "P0/P1/P2" = how much of the new UI blocks on it (P0 blocks a core floor; P1 degrades a floor; P2 is future/optional).

---

## Ask 1 — Agent working-chatter substrate ("Office Chatter") — P1

**Product intent:** Floor 3 shows the org's working chatter as a live, Slack-shaped channel viewport (channels ≈ workstreams; agents post short action/coordination notes as they work; ExecAss replies; the owner may speak but never has to). It is ambient observation — anything needing the owner still arrives only via typed attention on the Office floor.

**What the frontend needs:** a channel-shaped, subscribable stream: channel list, messages (sender identity, timestamp, text, optional thread ref/attachment ref), unread state, and a send path for the owner. Read-heavy; presentation is mine.

**Agreed direction:** CarsinOS Agent Mail already exists and remains the communication substrate and authority. Office Chatter is a focused view over Agent Mail rooms, preferably with a stable room-per-workstream mapping. The owner's Rust COLACK project (`Z:\COLACK`) is available as a code and interaction-pattern reference only; it is not a required label, connector, Postgres dependency, or second message store.

Current Agent Mail does not naturally produce a safe stream of run milestones, and ordinary mail bodies/metadata are not sufficient redaction for an ambient display. Add a small producer that deliberately authors short safe-payload working notes from approved lifecycle milestones. It must never copy raw tool output, secrets, arbitrary transcripts, or caller-controlled identity claims into ambient chatter. Threading and typing presence are optional capabilities; the frontend must render them only when actually supported.

**Fallback if deferred:** Floor 3 ships with Reef only; chatter panel shows a tasteful "not wired yet" state. No dual truth invented.

## Ask 2 — Floor presence projection (the Reef) — P1

**Product intent:** ambient aquarium of the crew: who is "out," a coarse current-task label, and a health mood (healthy / busy / recovering / idle / offline). Click → short report + deep link. Coarse presence only, never message content.

**What the frontend needs:** per-agent: id, display name, employment kind (see Ask 4), coarse activity label (human-safe string), mood enum, active delegation/session ref for deep link. Poll or WS — either fine; ~5s freshness is plenty.

**Agreed direction:** add a small read-only floor-presence projection rather than making the frontend stitch endpoints and invent activity truth. It should derive from authoritative agent/session/run/job/delegation facts, expose only coarse safe labels and deep links, and distinguish `idle`/`unknown` from a genuinely observed `offline` state. Polling is sufficient initially; no new control authority or lifecycle engine is created.

## Ask 3 — Boss board-moves ping ExecAss — P2

**Product intent:** in the Trenches, when the owner drags/moves a board card, ExecAss should "hear about it" as intent (owner moved X to column Y) rather than the org silently diverging from what ExecAss believes.

**Agreed direction:** board mutations already emit general websocket events, but ExecAss does not durably ingest them. The frontend must **not** perform a second `/execass/intake` request alongside the board mutation: the two requests can partially succeed, and attaching intake to a delegation is a real plan amendment rather than a harmless note. Add a durable server-side board-observation/outbox seam for linked work. ExecAss may consume that observation as evidence/context; the board event itself does not become action authority or a second lifecycle truth.

## Ask 4 — Staff vs. task-worker distinction — P1

**Product intent:** Teams becomes the staff directory: permanent staff only (ExecAss + wired-in model staff, e.g. Claude/Vale/Lyra). Ephemeral task-workers render nested *inside* their responsible staff member's card, and per-staff memory drill-in lives there too ("what does Claude remember?"). This is organizational display data only; it introduces no payroll, salary, payment, or financial subsystem.

**What the frontend needs on agent records:** (a) an employment kind (permanent staff vs. task-worker), (b) parent/employer agent ref for workers. Today I can only heuristic this from `reports_to` + naming, which will misfile someone eventually.

## Ask 5 — Briefing prose (ExecAss's morning voice) — P2

**Product intent:** the Office opens with a short assistant-written brief ("Morning, boss. Quiet night — two things for you…") derived from the summary projection. Structured data is fully covered by `GET /summary`; the question is only the prose.

**Agreed direction:** the frontend composes deterministic prose from the authoritative projection. No backend composer is required. Pure formatter tests cover empty, attention, failed, recovering, partial, and ordinary-progress summaries. No fabricated activity: the brief only restates projection facts and links back to them.

## Ask 6 — Incident posture signal — P2 (likely already covered — please confirm)

**Product intent:** the calm office changes posture when something is genuinely wrong (fire alarm: banner + walk-me-there). Derive posture client-side from existing authoritative signals: gateway/runtime health, breaker and connector state, `execass.v1.receipt.integrity_failed`, faulted `execass.v1.runtime_host.changed`, all actionable `summary.needs_you` items, failed/partially-completed summary outcomes, and stop-all/drain state. Intentional draining/stopped work and ordinary external waiting are not fire alarms. No new severity taxonomy or backend authority is needed.

## Ask 7 — Later / concepts (no action now)

- **"Call the Tech":** an MCP jack-in point in the Basement where a model (Dex/Claude/other) can inspect machinery on request and answer in plain words. Owner loves it; concept-stage; backend/MCP story is yours whenever.
- **Server-side sync of themes/layouts:** frontend persists locally (Tauri store) for now; a small user-prefs blob endpoint would enable multi-device later.
- **Notification history:** "While you were out" tray currently builds on client-observed events + `notification.scheduled` + summary ack revisions. If a queryable notification history exists/lands someday, I'll consume it.

---

## Explicit non-asks

No changes to: decision/confirmation semantics, proofs, run control, policy authority path, summary projection shape, WS resume protocol, error catalog. No spending/finance/tenant/role concepts. No second approval engine, scheduler, or receipt authority. The v1.1 contract as shipped is the foundation and it is sufficient for Floors 4, 2, and B in their entirety.

**Parallelization note for the owner:** nothing here blocks frontend start. P0–P2 of the design doc (foundations, plumbing, the Office) run entirely on the existing contract. Asks 1–2 gate only Floor 3's live data; Ask 4 gates only truthful Staff Directory nesting. Until Ask 4 lands, show current agents as staff and omit task-worker nesting rather than guessing from names or `reports_to`.
