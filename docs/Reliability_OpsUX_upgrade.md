# CarsinOS Mission Control: Reliability + Ops UX Upgrade (High-Level Spec)

Date: 2026-03-04  
Scope: Mission Control UI (single-user, desktop-first thin client to carsinOS Gateway)
Execution blockerboard: [Reliability_OpsUX_BLOCKERBOARD.md](./Reliability_OpsUX_BLOCKERBOARD.md)

## 0) Why we're doing this

Mission Control is operational software. When it breaks, the operator loses visibility/control. We need:

- A real test gate (unit + E2E) to catch breakages early.
- A Live Feed view that is always accessible (not buried in a tab).
- Crash-proof UI so one bad component does not blank the entire app.
- Optional cost/token charts if the Gateway can provide usage data.

## 1) Goals (what "good" looks like)

1. Confidence to ship: One command/CI job reliably says "safe to merge/release."  
2. Always-visible situational awareness: Operator can see "what is happening right now" from any tab.  
3. No white-screen-of-death: If a tab crashes, user gets a friendly recovery screen, not a dead app.  
4. Budget visibility (optional): If usage data exists, show trends + "what is expensive" clearly.

## 2) Non-goals (explicitly out of scope)

- Multi-user auth/RBAC (single-user app).
- Building a local DB-backed "all-in-one" Mission Control server (we remain a gateway client).
- Shareable links/routing deep-links (not needed for single-user).

---

## 3) Deliverable A - Test Gate (Unit + E2E)

### A1) "Quality Gate" concept

Add one quality gate entrypoint (example: `quality:gate`) with profiles:

- `quality:gate --profile=pr`
  - lint
  - typecheck
  - unit tests
  - core E2E suite (browser runtime)
  - web build sanity check
- `quality:gate --profile=release`
  - everything in `pr`
  - full E2E suite
  - Tauri smoke E2E subset (desktop runtime)
  - Tauri build sanity check

This gate is required for:

- PR CI checks (block merge on failure)
- Release workflow (block shipping on failure)

Emergency release rule (explicit): bypass is allowed only for critical hotfixes with operator approval plus written incident ticket and required follow-up gate rerun within 24 hours.

Build sanity definition (explicit):

- Web sanity: production web bundle completes with no compile errors.
- Tauri sanity: desktop bundle/build step completes with no compile errors.

### A2) Unit tests (what they must cover)

Unit tests should focus on logic that breaks silently:

- URL/token/runtime logic (connection settings behavior in web vs Tauri mode)
- Board/card manipulation logic (optimistic move ordering/positions)
- WebSocket event parsing + reconnect rules (malformed frames, reconnect backoff/cap)
- Event summarization logic (domain filters, summary strings, heartbeat hiding)
- Error boundary behavior (captures error, shows fallback, reset/retry works)
- Redaction helpers used by debug/copy surfaces

### A3) E2E tests (what they must cover)

E2E runs browser-driven against a deterministic stub/mock gateway + websocket server. Release profile additionally runs a Tauri smoke subset.

Selector policy (explicit):

- Prefer semantic role/name selectors where stable.
- Use `data-testid` for unstable/repeated interactive elements.
- Avoid timing sleeps; wait on visible UI state or deterministic network/mock events.

Required E2E coverage by phase:

1. Phase 1 core E2E (required in `pr` profile)
   - First run onboarding
     - With no connection, wizard opens.
     - Dismiss works and respects the 24-hour behavior.
     - Wizard can be reopened from UI.
   - Connect + baseline load
     - Enter gateway URL + token (or env token) -> app loads boards/agents.
     - Disconnected/reconnecting states show correctly when gateway is down.
   - Crash-proofing sanity
     - Force a controlled crash in one tab -> fallback UI appears and recovery action works.
2. Phase 2 E2E additions
   - Live Feed drawer open from any tab -> events appear, unread behavior works, pause behavior works.
3. Phase 3 E2E additions
   - Boards core workflow (create/move/run card with persistence and refresh)
   - Focus approvals workflow (approve/deny updates list + counts)

Release profile must include at least: onboarding, connect baseline, one forced crash recovery, one reconnect flap scenario in Tauri runtime.

### A4) Definition of Done (tests)

- Quality gate runs in CI and locally.
- Tests are stable (no random failures).
- E2E uses stable selectors and deterministic waits.
- Adding a new tab/major feature requires at least one unit test or E2E assertion touching it.
- Section 7 checklist is mandatory acceptance criteria, but scoped by phase:
  - Each phase must satisfy only the Section 7 bullets tagged to that phase (plus any explicitly carried-over open items).
  - Every scoped bullet must map to at least one automated assertion (unit, integration, or E2E).

---

## 4) Deliverable B - Live Feed Sidebar/Drawer (Always Accessible)

### B1) UX intent

A right-side drawer/sidebar openable from anywhere, showing the most important "right now" signals:

- Connection status (gateway health + websocket state)
- Recent high-signal events (filtered, summarized)
- Recent notifications/toasts (history)
- Counts: approvals pending, breakers open, mail unread (minimum)

### B1.1) Event envelope contract (minimum)

Live Feed expects normalized event metadata from gateway/websocket pipeline:

- `event_id` (stable unique id)
- `timestamp_utc` (ISO8601 UTC)
- `domain` (`approvals|jobs|boards|mail|channels|system|other`)
- `severity` (`critical|high|normal|low`)
- `summary` (safe short text)
- `payload_redacted` (optional expanded payload, already scrubbed)

If event metadata is incomplete, UI must degrade safely (show as `domain=other`, `severity=normal`, summarized fallback text).

### B2) Content rules (reduce noise)

- Default view hides spammy items (for example, heartbeats).
- Show summaries by default; full payload only on explicit expand/click.
- Include quick filters: `All / Approvals / Jobs / Boards / Mail / Channels / System`.
- Include `Pause` mode (stop auto-scroll and stop auto-mark-read while paused).

### B3) Behavior rules

- Unread badge appears when drawer is closed.
- `Mark all read` is explicit and reversible: keep a 5-minute undo window.
- `Clear` acts as soft clear only: events remain recoverable in session history for 30 minutes.
- Drawer must not steal focus unexpectedly; keyboard shortcut toggle is allowed.

Recoverability storage contract (explicit):

- In-memory feed list stays capped for responsiveness.
- The 30-minute recoverability promise is backed by a session-level persisted recovery log (local durable store), not only in-memory buffers.
- If persisted recovery storage is unavailable/disabled, UI must explicitly downgrade wording to memory-only recoverability and show current effective retention window.

Incident mode behavior (explicit):

- Incident mode can be entered either:
  - manually by operator toggle, or
  - automatically when gateway health is degraded for >30 seconds or any `critical` event arrives.
- Incident mode exits only when operator turns it off or when system remains healthy for 5 continuous minutes.
- In incident mode, default feed filter is `severity in {critical, high}`; operator can still broaden manually.
- Manual override precedence:
  - If operator manually turns incident mode off, auto-triggers are suppressed for 10 minutes (cooldown).
  - Cooldown exception: a new `critical` event can immediately re-enter incident mode.
  - If operator manually turns incident mode on, it remains on until operator turns it off.

### B4) Performance constraints

- Must remain responsive under event bursts.
- Reference burst targets for validation:
  - sustained load: 5 events/second for 10 minutes
  - spike load: 20 events/second for 30 seconds
- UI interaction SLO on reference operator hardware (p95):
  - drawer open/close <= 200 ms
  - filter toggle/pause toggle <= 200 ms
- Keep bounded memory with deterministic caps:
  - max retained feed events in-memory: 2,000
  - max rendered rows at once: 300 (virtualized list)
- These caps apply to active in-memory/rendered feed state; they do not reduce the separate 30-minute persisted recovery log contract above.
- Pause behavior under burst:
  - ingest continues into bounded ring buffer
  - unread count increments
  - no auto-scroll or implicit read marking while paused
- Overflow policy: drop oldest `low` then `normal` first; never drop `critical` until all lower severities are exhausted.
- Ordering policy: sort by `timestamp_utc`, tie-break by arrival index.

### B5) Definition of Done (live feed)

- Accessible from every tab.
- Shows correct counts and recent events.
- Handles gateway disconnect/reconnect gracefully.
- Meets Section B4 burst targets and p95 interaction SLOs.
- During burst tests, no `critical` events are dropped.
- Includes automated tests for pause semantics, unread behavior, reconnect flaps, and burst overflow policy.

---

## 5) Deliverable C - Crash-Proofing UI (Nice Error Screens Per Tab)

### C1) The problem

A runtime exception inside a tab can take down the view and leave the operator stuck.

### C2) Required behavior

Add error handling so failures degrade gracefully:

- Per-tab error boundary: if Boards crashes, only Boards shows an error screen.
- Global error boundary: if something catastrophic happens, show a global recovery screen.
- Fallback loop guard: if fallback UI itself throws repeatedly, force safe-mode global screen with reload action.

### C3) Error screen UX (operator-friendly)

When a tab crashes, show:

- Human text: "This tab crashed."
- Primary actions:
  - Retry (re-render tab)
  - Reset tab state (clear local tab UI state only)
  - Reload app (last resort)
- Debug affordances:
  - Copy error details (message + stack)
  - Show last N events (for correlation)

Reset tab state scope (explicit):

- Clears: ephemeral tab-local state (filters, transient form state, local selection).
- Preserves: connection settings, auth token references, global app settings, other tabs, persisted server data.

Redaction rules (explicit):

- Copied/debug output must redact secrets by default (tokens, API keys, OAuth codes/tokens, auth headers, cookie-like values).
- `Show last N events` uses redacted summaries by default; raw payload view requires explicit operator action and still applies secret scrubbing.
- Onboarding exception: wizard token-entry fields may be plaintext during active entry by design; once setup is applied/exited, token values must be masked or removed from visible UI and persisted only in approved secret storage paths.

### C4) Definition of Done (crash-proofing)

- Any single tab can crash without killing rest of app shell/navigation.
- Operator always has clear recovery path.
- E2E includes at least one forced crash -> recover test.
- E2E includes one global-boundary recovery test path.

---

## 6) Deliverable D - Cost/Token Charts (ONLY if Gateway exposes usage)

### D1) Dependency

This feature ships only if gateway provides safe, aggregated usage metrics without exposing sensitive prompt contents.

Minimum usage contract required:

- `window_start_utc`, `window_end_utc`
- `timezone` (IANA string used for bucket boundaries)
- `currency` (ISO code, for example `USD`)
- `estimated_cost_total`
- `token_input_total`, `token_output_total`
- `by_agent[]` (agent id/name + cost/tokens)
- `by_model[]` (model id + cost/tokens)
- `by_job[]` (optional, but required to enable job-spike correlation UI)
- `by_card[]` (optional, but required to enable card-spike correlation UI)
- `updated_at_utc` (freshness timestamp)

If any required field is missing or invalid, UI must disable charts and show clear "Not available" state.
If `by_job[]`/`by_card[]` are absent, UI must hide correlation subviews and clearly label correlation data as unavailable (without disabling the rest of cost charts).

### D2) What the UI should answer

- "What is my spend today/this week?"
- "Which agent/model is most expensive?"
- "Are costs trending up/down?"
- "Any spikes correlated with specific jobs/cards?"

Time-window definitions (explicit):

- `today` and `this week` are computed in operator timezone from gateway contract.
- Week starts Monday 00:00 local timezone unless gateway contract says otherwise.

### D3) UX requirements (high level)

- Small summary widget (today/week totals + trend).
- Breakdown view (by agent, by model, by time).
- Clear data freshness indicator (`updated_at_utc` rendered as local time).
- Budget warning thresholds (optional): simple, obvious, non-spammy.
- Staleness behavior: if data age exceeds 15 minutes, show stale warning; if age exceeds 60 minutes, suppress trend claims and show limited state.

### D4) Definition of Done (cost charts)

- If gateway has data: charts render, remain understandable, and update correctly.
- If gateway lacks/invalid data: UI clearly shows "Not available" and does not mislead.
- Includes automated tests for available-data and unavailable-data states.

---

## 7) Cross-Cutting QA / Edge Cases Checklist (Integrated from review passes, phase-scoped)

Checklist scoping rule:

- Bullets are tagged by owning phase (`P1`..`P4`).
- A phase is blocked only by bullets owned by that phase (plus carried-over unresolved bullets).
- By the end of its owning phase, each bullet must be covered by at least one automated assertion tracked in test artifacts.

Reliability / regressions

- [P1] Gateway URL changes mid-session (feed + tabs update without stale data).
- [P1] WebSocket flaps (rapid disconnect/reconnect) without duplicating/exploding state.
- [P1] Malformed WS events do not crash UI (drop safely + optionally log).
- [P2] Very large event payloads do not freeze UI (summarize; expand on demand).
- [P2] Pause feed actually pauses (no sneaky auto-scroll or read-marking).
- [P3] Boards create/move/run workflow persists across refresh/reload.
- [P3] Reconnect-edge scenarios (rapid flap + malformed frames) recover to connected without exploding state buffers.

State + recovery

- [P1] Tab reset does not wipe global app settings unintentionally.
- [P1] Error recovery does not cause infinite crash loops.
- [P2] Clear notifications does not break live feed counts and supports undo/restore window.
- [P3] Focus approvals approve/deny actions update queue content and pending counts deterministically.

Security / privacy

- [P2] Live Feed never shows raw secrets by default.
- [P1] Copy/debug features are explicit and scrub secrets.
- [P1] Wizard token-entry fields can be plaintext only during active onboarding entry; outside onboarding entry state, tokens must not be visible.
- [P1] If screenshots are taken, default view should not expose tokens (except operator-controlled active onboarding token-entry field).

Test stability

- [P1] E2E avoids racey sleep patterns; waits on visible states or deterministic mocks.
- [P1] E2E runs against deterministic stub gateway + websocket.
- [P1] Tests do not require real provider OAuth flows (mock/stub those paths).
- [P3] Tauri smoke includes one representative operator parity path (onboarding + boards + focus).

Acceptance binding (explicit): each scoped bullet above must be covered by at least one automated assertion tracked in test plan artifacts by the end of its owning phase.

---

## 8) Phasing (to keep risk down)

Phase 1: test harness + crash-proofing foundations + core E2E set (onboarding/connect/crash recovery).  
Phase 2: Live Feed v1 (summaries + counts + pause + incident-mode defaults) + Live Feed E2E.  
Phase 3: expand E2E coverage for full operator workflows (boards/focus/reconnect-edge suites).  
Phase 4 (optional): cost/token charts if gateway supports usage data contract.

Phase completion rule: a phase is done only when its scoped quality-gate profile is green and its Section 7 bullets tagged for that phase are covered (plus any carried-over unresolved bullets).

---

## 9) Rollout / Backout Guardrails

- Gate new Live Feed and crash-recovery UI behind runtime feature flags for controlled rollout.
- Provide explicit kill-switch config for:
  - live feed drawer
  - incident-mode auto-trigger
  - cost charts module
- If rollout causes perf or correctness regression:
  - disable affected flag immediately
  - keep core shell/tabs operational
  - document incident + follow-up test gap before re-enable
- Release checklist must include backout command path and owner on-call assignment.

---

## Caveman summary (handoff)

- Add one "big red button" quality gate with clear PR vs release profiles.
- Make E2E/runtime scope explicit so desktop regressions cannot slip through.
- Add Live Feed from anywhere with explicit incident + severity rules and burst-safe buffering.
- Add crash armor so one tab failing does not kill the app; include safe recovery + secret-safe debug output.
- Add cost charts only when gateway provides a clear safe metrics contract; otherwise show "Not available" clearly.
