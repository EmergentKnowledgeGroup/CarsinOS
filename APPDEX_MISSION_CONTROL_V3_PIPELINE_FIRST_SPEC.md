# Mission Control v3 (Pipeline-First) Plan for carsinOS

Date: 2026-02-27  
Owner: AppDex  
Status: Draft (implementation-ready)

## Plain-English Goal
Build a brand-new Mission Control UI (no reuse of the current Electron MC UI) that connects to the **carsinOS gateway** and gives you the same “command center” power as the MC-pipeline concept:
- tasks board (who is doing what)
- content pipeline (Trello-style production flow)
- calendar (scheduled/cron jobs you can trust)
- memory (search + browse)
- team (agents + roles + status)
- approvals (human-in-the-loop safety)

## Hard Requirements (From Your Ask)
1. **Rewrite UI from scratch**. Do not adapt/re-skin the current MC UI.
2. **Pipeline-first**: prioritize boards + scheduling over “cockpit widgets”.
3. **Dual-agent operation**: Lyra + Claude can both be “active” at the same time; operator can assign work to either and swap defaults without breaking the other.
4. **carsinOS is the backend**: MC is a client app that connects over HTTP + WebSocket; no Convex/extra backend.
5. **Guardrails by default**: avoid runaway loops and unbounded prompt/context growth.

## What “Dual Agent” Means in carsinOS Terms
- carsinOS already supports **multiple agents** via `agent_id` on sessions and routing rules.
- Mission Control must treat **Lyra** and **Claude** as two first-class agents:
  - separate default provider/auth profile ordering per agent
  - separate sessions (work streams)
  - visible side-by-side status (no “hidden active profile” confusion)

## Product Surface (MVP Pages)
1. **Tasks**: Kanban board for work items, owner = human or agent (Lyra/Claude), status columns.
2. **Content**: Kanban board with columns (Ideas -> Scripting -> Thumbnail -> Filming -> Editing -> Published). Each card holds script text + assets.
3. **Calendar**: week view of carsinOS scheduled jobs, “always running” tasks, next-up list.
4. **Memory**: notes list + search (and later: MNO context explainability hooks).
5. **Team**: agent roster (role + status + current workload) + quick assignment controls.
6. **Approvals**: pending approvals queue with approve/deny + audit trail.

Office view is optional and explicitly non-blocking.

## Backend Gaps to Close (carsinOS)
carsinOS already has: sessions, runs, approvals, jobs (scheduler), notes, channels, auth profiles, runtime config, WS events.

Missing for pipeline-first MC:
1. **Agents API**: list/create/update agents (the DB table exists; API does not).
2. **Boards API**: kanban boards/columns/cards for Tasks + Content Pipeline.
3. **Card assets**: upload and attach images/files to content cards (or store script text directly in card fields).
4. **Realtime updates**: board change events over WS (or poll, but WS is preferred).

## Build Gates (Clear “Stop/Go” Checkpoints)
### Gate A: Daily Driver MVP (Manual Control Works)
Pass when:
- Tasks + Content boards work end-to-end (create/edit/move cards; assign Lyra/Claude).
- Calendar shows jobs + next-run accuracy; operator can run/pause jobs.
- Approvals can be resolved reliably.
- No action requires editing source code or restarting the gateway.

### Gate B: Safe Automation (Optional, But Controlled)
Pass when:
- A scheduled job can move a content card forward (ex: Scripting -> Thumbnail) and trigger a run.
- Automation has caps (max runs per day, max retries, breaker on repeated failures).
- Operator can disable automation per-board/per-column quickly.

### Gate C: Mobile Companion (Operator Anywhere)
Pass when:
- Android app (or PWA) can receive alerts + resolve approvals and view boards read-only.
- Strong auth + clear audit entries.

## Relationship to Connector Apps
- **Mission Control apps** are operator dashboards (read/act/approve).
- **Connector apps** are remote “hands” (run allowlisted tools/capabilities on other devices).
They are separate tracks but share the same backend (carsinOS).

Reference docs already created:
- `APPDEX_RMP_CONNECTORS_SPEC.md`
- `APPDEX_CONNECTOR_HUB_EXECUTABLE_TICKET_BACKLOG.md`

