# AppDex Executable Ticket Backlog: Mission Control v3 (Pipeline-First)

Date: 2026-02-27  
Owner: AppDex  
Scope: New Mission Control UI + missing carsinOS backend surfaces required for pipeline-first operation.

Spec: `APPDEX_MISSION_CONTROL_V3_PIPELINE_FIRST_SPEC.md`

## Phase Order (Strict)
1. P0: Daily Driver MVP (manual control works, dual-agent works).
2. P1: Safe automation (optional, controlled).
3. P2: Mobile companion + polish.

## P0 Tickets (Daily Driver MVP)

### MC3-BE-001 Agents API + Seed “Lyra” and “Claude”
Priority: P0  
Goal: Mission Control can list and manage agents without DB surgery.  
Deliver:
1. Add API endpoints to list/create/update agents (the DB table exists).
2. Ensure agents `lyra` and `claude` exist by default (seed on bootstrap if missing).
3. Expose per-agent provider profile ordering controls (already exists) in a clean UI-ready shape.
Acceptance:
1. Operator can create a session for `lyra` or `claude` from UI without errors.
2. Agent list and agent details are readable without touching SQLite directly.

### MC3-BE-002 Kanban Boards (Tasks + Content Pipeline)
Priority: P0  
Goal: A Trello-style board system that powers both “Tasks” and “Content”.  
Deliver:
1. Add DB tables for:
   - boards
   - columns
   - cards
   - (optional) card comments / audit trail
2. Add CRUD APIs:
   - list boards, get board
   - create/edit/move cards (including ordering within a column)
   - assign owner (`human` or agent_id)
3. Provide two built-in boards on first run:
   - Tasks board (Backlog/In Progress/Review/Done)
   - Content board (Ideas/Scripting/Thumbnail/Filming/Editing/Published)
Acceptance:
1. Cards can be created, edited, assigned, moved, and persisted across restart.
2. A board can be reloaded and rendered from APIs only (no hidden local state).

### MC3-BE-003 Card Assets (Script + Images)
Priority: P0  
Goal: Content cards can hold a script and attach images (thumbnail drafts).  
Deliver:
1. Add a safe upload endpoint for card assets (image/file) with size caps.
2. Store assets under the gateway attachments directory with DB metadata.
3. Allow cards to reference:
   - `script_markdown` (or equivalent field)
   - `asset_refs[]` (attachment ids or stable paths)
Acceptance:
1. Uploading an image attaches to the card and survives restart.
2. No path traversal; no arbitrary file overwrite.

### MC3-BE-004 Realtime Events for Boards
Priority: P0  
Goal: Mission Control updates live like Trello.  
Deliver:
1. Emit WS events for card create/update/move and board/column changes.
2. Include minimal payloads suitable for UI diffing.
Acceptance:
1. Two clients open on the same board see updates within 1 second (local).

### MC3-BE-005 Card “Run” Hook (Optional, Minimal)
Priority: P0  
Goal: A card can trigger a carsinOS run with the assigned agent.  
Deliver:
1. Add a “run this card” action that:
   - creates/uses a session linked to the card (per-agent)
   - writes a message (“please do X”) and triggers `POST /sessions/{id}/runs`
2. Persist linkage: card -> session_id -> latest run_id.
Acceptance:
1. Clicking “Run” yields a visible run record and does not block board usage on failure.

### MC3-UI-000 Choose UI Implementation Track (One Decision, Then Commit)
Priority: P0  
Goal: Stop thrash; ship UI quickly.  
Decision options:
1. **Web UI (recommended for fastest “Trello board” quality)**: React + drag/drop + connects to carsinOS HTTP/WS.
2. Rust desktop UI (slower to reach Trello polish; fine if you want all-Rust).
Acceptance:
1. A skeleton app can auth to the gateway and render a board list.

### MC3-UI-001 Mission Control Shell + Navigation
Priority: P0  
Goal: New UI baseline with the pages from the pipeline spec.  
Deliver:
1. Left nav: Tasks, Content, Calendar, Memory, Team, Approvals, Settings.
2. Top bar: search + agent indicators (Lyra + Claude).
Acceptance:
1. No page requires scrolling to access primary actions.

### MC3-UI-002 Tasks Board (Kanban)
Priority: P0  
Deliver:
1. Kanban columns + drag/drop cards.
2. Card detail drawer: description, owner, due date, tags, linked session/run.
Acceptance:
1. Moving a card updates other clients in realtime.

### MC3-UI-003 Content Pipeline Board (Kanban)
Priority: P0  
Deliver:
1. Stages: Ideas -> Scripting -> Thumbnail -> Filming -> Editing -> Published.
2. Card fields: script markdown, attachments.
Acceptance:
1. Attachments render reliably and don’t break board performance.

### MC3-UI-004 Calendar (Jobs + Next Up)
Priority: P0  
Deliver:
1. Week view of scheduled jobs from carsinOS.
2. “Always running” lane + “Next up” list.
3. Run now / pause / resume actions.
Acceptance:
1. Operator can verify scheduled tasks exist and are running as expected.

### MC3-UI-005 Memory (Notes + Search)
Priority: P0  
Deliver:
1. Notes list + detail view.
2. Global search for notes (and later: MNO explainability).
Acceptance:
1. Search results are fast and stable on large note sets.

### MC3-UI-006 Team (Agents)
Priority: P0  
Deliver:
1. List agents + roles + status signals.
2. Per-agent quick actions: “make default for this board”, “connectivity/health snapshot”.
Acceptance:
1. Lyra and Claude are always visible and both “active” (no hidden active profile confusion).

### MC3-UI-007 Approvals (Human-in-the-Loop)
Priority: P0  
Deliver:
1. Pending approvals queue with approve/deny.
2. Audit summary visible per approval.
Acceptance:
1. Approvals resolution is reliable and idempotent.

## P1 Tickets (Safe Automation)

### MC3-AUTO-001 Column Automation Rules (Opt-In)
Priority: P1  
Deliver:
1. Per-column automation config (disabled by default):
   - schedule (daily/weekly)
   - which agent to use
   - max attempts + breaker window
2. “Run automation now” button for operators.
Acceptance:
1. Automation can be paused instantly without code changes.

### MC3-AUTO-002 Content Automation MVP (Script -> Thumbnail)
Priority: P1  
Deliver:
1. Job that finds cards in “Scripting” and generates/updates script.
2. Job that generates a thumbnail image draft and attaches it to the card.
Acceptance:
1. Automation never loops infinitely (caps + breaker + audit).

## P2 Tickets (Mobile Companion)

### MC3-MOB-001 Mobile Read-Act Surface
Priority: P2  
Deliver:
1. Read-only boards + approvals + alerts on mobile.
2. Approve/deny from phone with strong auth.
Acceptance:
1. Approvals can be resolved from mobile without desktop.

## Guardrails (Unskippable)
1. Ship P0 backend endpoints before polishing UI.
2. Every automation must have caps, breaker behavior, and audit rows.
3. No secrets/tokens in logs or UI responses (write-only secret refs).

