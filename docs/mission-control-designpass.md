# Mission Control Design Pass

**Author:** Claude (Opus 4.6)
**Date:** 2026-02-27
**Scope:** Full UI/UX audit of `apps/mission-control/src/App.tsx` + `styles.css`
**Status:** Proposal — no code changes yet

---

## 1. What CarsinOS Actually Is (and Why the UI Must Match)

CarsinOS is a Rust-native AI gateway and agent orchestration platform. The operator sits at **Mission Control** and manages:

- **AI agent fleets** running scheduled jobs, processing mail, executing tool calls
- **Multi-channel communications** (Discord, Telegram, GUI)
- **Approval gates** where humans decide what agents can do
- **Kanban boards** for task orchestration with agent-owned cards
- **Real-time event streams** from a WebSocket gateway
- **Circuit breakers, scheduler health, and incident response**

This is an **ops console**. The person using this is monitoring autonomous agents, making split-second approval decisions, triaging failures, and coordinating AI-to-AI communication. The UI needs to feel like the bridge of a ship, not a Notion template.

The name "Mission Control" is right there. Lean into it.

---

## 2. Current State Audit

### 2.1 What's Actually Built (It's a Lot)

The frontend is a **3,745-line monolith** (`App.tsx`) with 7 fully functional views:

| Tab | What It Does | Complexity |
|-----|-------------|------------|
| **Boards** | Kanban with virtualized columns, drag-and-drop, card editor drawer, asset upload/preview | High |
| **Calendar** | Weekly schedule view with always-running/next-up lanes, full scheduler matrix table | Medium |
| **Focus** | Operator attention queue with severity-filtered items, inline approve/deny/retry/reconnect | Medium |
| **Events** | Live WebSocket event stream with JSON payload inspection, heartbeat filter | Low-Medium |
| **Agent Mail** | 3-column mail client: thread list + message stream + compose/lease panel, file leases, thread summarization to memory notes | High |
| **Chatrooms** | Room-based messaging with reactions, moderation panel, workspace lease reservations, bulk ack | High |
| **Cockpit** | Configurable dashboard with 9 widget types, drag-to-reorder, resize, multi-page layouts, import/export JSON | High |

Plus infrastructure: connection management, WebSocket lifecycle, Tauri keychain integration, optimistic board updates, real-time event-driven refreshes.

**This is genuinely feature-rich.** The bones are solid — the data plumbing, state management, and API integration are well-structured. The problem is purely visual and experiential.

### 2.2 What's Wrong: The Honest List

#### CRITICAL: No Dark Mode

```css
--bg: #f1ede6;        /* warm beige */
--surface: #fffdfa;    /* near-white */
```

This is an operator console. People stare at this for hours. Every other ops tool (Grafana, Datadog, Vercel, Linear, Bloomberg Terminal) defaults to dark for good reason. The current palette looks like a baking recipe app.

**Verdict:** Dark mode isn't a nice-to-have — it's table stakes.

#### The "Parchment Problem"

The entire color system is built around warm cream tones: `#f1ede6`, `#fff7e8`, `#faf5ee`, `#fffaf3`, `#fff5e7`, `#fffdf8`, `#fff0e0`, `#fff0df`. Everything is a slightly different shade of warm white. This creates:

- **Zero visual hierarchy** — every panel, card, widget, and lane looks the same
- **No depth or layering** — surfaces don't separate from background
- **Fatigue** — warm tones strain eyes in extended sessions
- **No personality** — could be any app

#### Typography: Safe to the Point of Invisible

```css
font-family: "IBM Plex Sans", sans-serif;  /* body */
font-family: Unbounded, sans-serif;        /* headings */
```

IBM Plex Sans is a fine workhorse font but it's the "oatmeal" of type choices — nutritious, forgettable. Unbounded is actually interesting (geometric, bold, futuristic) but it's barely used — just a few `h2`/`h3` headers.

The type scale is also flat:
- Everything clusters between `0.68rem` and `0.92rem`
- No dramatic size contrasts
- Labels are ubiquitously `text-transform: uppercase; letter-spacing: 0.04-0.17em` — when everything is uppercased, nothing stands out

#### Borders Everywhere, Depth Nowhere

Every single element has `border: 1px solid #d-something`. Borders on panels, borders on cards, borders on lane panels, borders on widgets, borders on list items, borders on thread items, borders on focus items, borders on health grids. The entire UI is a grid of bordered rectangles.

The `box-shadow: 0 16px 34px rgba(20, 18, 13, 0.12)` is applied to top-level surfaces but is too subtle against the light background. Panels don't "lift" off the page.

#### Zero Motion

The only animation in the entire app:

```css
button:hover {
  transform: translateY(-1px);
  filter: brightness(1.04);
}
```

That's it. No:
- Tab transition animations
- Card entrance/exit animations
- Loading states or skeleton screens
- Event stream item slide-in
- Notification enter/exit
- Drag ghost styling
- Focus ring animations
- Panel expand/collapse transitions

#### Flat Information Hierarchy

All 7 views use the same visual treatment: rounded box with border. There's no way to tell at a glance whether you're looking at a high-severity approval, a routine event, a critical breaker, or an idle job. Severity chips exist (`chip-up`, `chip-down`, `chip-error`) but they're tiny text with tinted borders — not the visual alarm bells they should be.

#### No Icons

The entire UI is text-only. No iconography for:
- Tab navigation
- Widget types in the cockpit
- Status indicators (healthy/degraded/faulted)
- Mail actions (compose, ack, attach)
- Card ownership (agent vs human vs unassigned)
- Channel types (Discord, Telegram)

#### Notification System is Primitive

`{notice ? <div className="mc-notice">...` — a single notice banner that gets overwritten by the next notice. No:
- Toast stack
- Auto-dismiss timers
- Severity-based positioning
- Action buttons on notifications
- History

#### Event Stream is Raw JSON

```jsx
<pre>{JSON.stringify(event.payload, null, 2)}</pre>
```

This is a developer debug dump, not an operator view. Events should be visually parsed — showing the key entity, action, and important fields in a structured way, with the raw JSON available on expand.

#### Forms Have No Personality

Every input, select, and textarea uses the same rounded-corner treatment. No visual distinction between:
- Search inputs vs. data inputs
- Primary actions vs. secondary actions vs. danger actions
- Connection config (critical) vs. compose fields (routine)

### 2.3 What's Actually Good (Don't Throw This Away)

- **Virtualized lists** (`@tanstack/react-virtual`) on both board columns and card lists — smart
- **Optimistic updates** on card moves with rollback on failure
- **WebSocket-driven reactive state** — events trigger targeted refreshes
- **Cockpit layout persistence** to localStorage with sanitization
- **Debounced refresh queues** preventing API spam
- **File lease system** for multi-agent workspace coordination (genuinely novel UX)
- **Thread summarization to memory notes** — bridging comms and knowledge
- **The accent color** `#ff6a13` is actually excellent — warm amber/orange — it just needs a dark canvas to shine against
- **Unbounded font** is a strong display choice, just underutilized

---

## 3. Design Direction: "Dark Ops"

### 3.1 The Concept

Think: **Vercel's dashboard precision meets Bloomberg Terminal density meets Alien (1979) amber CRT glow.**

This is a command station. The operator is managing autonomous AI agents across multiple channels. The aesthetic should communicate: **control, awareness, precision, trust.**

Not sci-fi cosplay. Not gratuitous neon. Dark, functional, considered — where every visual choice earns its place by making the operator faster and more aware.

### 3.2 Color System

#### Dark Mode (Primary)

```
Background layers (darkest to lightest):
  --bg-void:      #0a0a0c        /* deepest layer, app chrome */
  --bg:           #101114        /* main background */
  --bg-raised:    #16171c        /* cards, panels */
  --bg-overlay:   #1c1d24        /* modals, drawers, floating elements */

Surface & borders:
  --surface:      #1e2028        /* widget bodies, list items */
  --surface-hover:#252730        /* interactive hover state */
  --line:         #2a2d38        /* subtle borders */
  --line-strong:  #3a3e4c        /* emphasized borders */

Text:
  --ink-primary:  #e8e6e1        /* primary text, high contrast */
  --ink-secondary:#8b8d95        /* labels, metadata, secondary */
  --ink-muted:    #5c5f69        /* placeholder, disabled */

Accent:
  --accent:       #ff6a13        /* kept from current — it's perfect */
  --accent-glow:  rgba(255, 106, 19, 0.15)  /* ambient glow behind active elements */
  --accent-ink:   #ffecd8        /* text on accent backgrounds */
  --accent-muted: #994010        /* de-emphasized accent */

Semantic:
  --ok:           #22c55e        /* brighter green, reads on dark */
  --ok-muted:     rgba(34, 197, 94, 0.12)
  --warn:         #f59e0b        /* amber warning */
  --warn-muted:   rgba(245, 158, 11, 0.12)
  --danger:       #ef4444        /* red, unmissable */
  --danger-muted: rgba(239, 68, 68, 0.12)
  --info:         #3b82f6        /* blue informational */
  --info-muted:   rgba(59, 130, 246, 0.12)
```

#### Light Mode (Optional, System-Preference Toggle)

Keep a refined version of the current light palette as `prefers-color-scheme: light`, but shift from warm beige to a cooler, crisper tone:

```
  --bg:           #f4f5f7        /* cool gray, not warm beige */
  --surface:      #ffffff
  --line:         #e2e4ea
  --ink-primary:  #0f1117
  --ink-secondary:#6b7280
```

### 3.3 Typography

#### Font Stack

Replace IBM Plex Sans body with something with more character. Keep Unbounded for display.

**Option A: JetBrains Mono + Unbounded**
```
Display / headings:  Unbounded (keep — it's excellent)
Body / UI:           "General Sans", "Satoshi", or "Plus Jakarta Sans"
Monospace / data:    "JetBrains Mono", "Berkeley Mono", "Geist Mono"
```

The monospace isn't just for code blocks — use it for **data values**: timestamps, IDs, counts, status labels, event types. This immediately separates "things you read" from "things you scan."

**Option B (if licensing is simpler): Geist + Geist Mono**
Vercel's Geist family is open source, beautifully designed, and has both sans and mono variants. Pairs well with Unbounded display headers.

#### Type Scale

Create actual contrast:

```
--type-display:   1.75rem    /* page titles, brand */
--type-heading:   1.15rem    /* section headers */
--type-subhead:   0.92rem    /* card titles, widget headers */
--type-body:      0.84rem    /* body text, descriptions */
--type-caption:   0.74rem    /* metadata, timestamps */
--type-micro:     0.66rem    /* badges, very small labels */
--type-mono-data: 0.80rem    /* monospace data values */
```

### 3.4 Component-Level Redesign Notes

#### Navigation (Tabs)

The current tab bar is a row of identical buttons. Replace with a **vertical sidebar rail** or a **segmented control bar** with:

- Icon + text label for each tab
- Active tab indicated by accent-colored left border (sidebar) or bottom bar (horizontal)
- Unread/alert badges on tabs (red dot on Focus when approvals pending, etc.)
- Keyboard shortcuts displayed as subtle hints (1-7 for tab switching)

Suggested icon concepts per tab:
```
Boards:     grid/kanban icon
Calendar:   calendar icon
Focus:      eye/target icon
Events:     pulse/waveform icon
Mail:       envelope icon
Chatrooms:  message-bubble icon
Cockpit:    dashboard/gauge icon
```

#### Topbar

Currently sparse — just brand name + three status chips. Make it the **persistent command bar**:

```
LEFT:    CarsinOS logo/wordmark (Unbounded, accent color)
CENTER:  Global search input (cmd+K to focus)
RIGHT:   Connection status cluster (ws dot + health dot + token lock)
         + quick-action button (incident mode toggle)
         + theme toggle (sun/moon)
```

#### Pinned Health Strip

This is one of the most important elements — it tells you the system posture at a glance. Currently it's a beige flex row of identically-styled boxes.

Redesign as a **status bar with personality**:

- Each stat gets a subtle colored background based on value (green-muted when healthy, danger-muted when troubled)
- Numbers displayed in monospace at larger weight
- Incident mode toggle should be a prominent switch with a visual state change (the whole strip's border glows danger-red when incident mode is ON)
- Consider micro-sparklines next to key metrics (approvals trending, breakers over time)

#### Board Cards

Currently:
```
┌──────────────────┐
│ Card Title       │
│ owner_kind  run: │
└──────────────────┘
```

Should be:
```
┌──────────────────────────┐
│ ▊ Card Title             │   ← left color bar indicates owner type
│   description preview... │      (amber=agent, blue=human, gray=unassigned)
│ ┌────┐ ┌────┐           │
│ │ tag │ │ tag │          │   ← tag chips with color
│ └────┘ └────┘           │
│ 🤖 agent-name  ⏱ due   │   ← icon-adorned metadata
│                run:xxxx  │
└──────────────────────────┘
```

Also:
- Drag ghost should have a tilted, shadow-elevated style
- Selected card should pulse with accent glow, not just a border change
- Run status should show a tiny progress indicator when a run is active

#### Event Stream

Replace raw JSON dump with structured event cards:

```
┌─ board.card.moved ────────── 2:34:12 PM ─┐
│ entity: board/ops-main                     │
│ card_id → column "In Progress"             │
│ [▸ Raw JSON]                               │  ← collapsed by default
└────────────────────────────────────────────┘
```

Color-code the left border by event domain:
- `board.*` — accent orange
- `job.*` — blue
- `approval.*` — yellow/amber
- `channel.*` — purple
- `agent_mail.*` — green
- `heartbeat.*` — dim gray

#### Mail & Chatrooms

The three-column mail layout is correct but needs visual differentiation:

- **Thread list sidebar**: slightly darker background, dense layout
- **Message stream center**: lighter surface, generous spacing, message bubbles with sender avatars (colored circles with initials)
- **Compose/leases panel**: distinct background, input-focused

Messages from different senders should have visually distinct treatments — left-aligned vs right-aligned, or different background tints.

Unread count badges should be red dots or accent-background pills, not the current subtle border tint.

#### Cockpit

This is the crown jewel view — a customizable dashboard. Currently all widgets look identical. Each widget type should have a subtle visual identity:

- **Health**: status-colored header bar
- **Focus**: pulsing indicator when items are critical
- **Breakers**: red-tinted header when breakers are open
- **Jobs**: timeline/schedule visual rather than just a list
- **Channels**: green/red dots for healthy/degraded
- **Events**: terminal-style monospace with scrolling feel

The widget arrangement controls (Up/Down/+/-/Remove) should be **hidden by default** and revealed on hover or via a "Edit Layout" toggle mode — they currently clutter every widget header.

### 3.5 Motion & Interaction Design

#### Principle: Purposeful Motion

Every animation should communicate something:

```css
/* Shared timing tokens */
--ease-out:    cubic-bezier(0.16, 1, 0.3, 1);    /* for entrances */
--ease-in-out: cubic-bezier(0.65, 0, 0.35, 1);   /* for state changes */
--duration-fast:   120ms;
--duration-normal: 200ms;
--duration-slow:   400ms;
```

#### Specific Animations

1. **Tab switches**: Cross-fade content with a 200ms opacity transition + subtle translateY
2. **Card drag**: Picked-up card scales to 1.03 with deeper shadow; drop zones highlight with dashed accent border
3. **Event stream**: New events slide in from top with opacity fade, 300ms stagger
4. **Notification toasts**: Slide in from top-right, auto-dismiss after 4s with progress bar
5. **Focus items**: High-severity items have a subtle pulse on their left color bar
6. **Connection status**: Smooth color transitions between states (idle → checking → connected)
7. **Button interactions**: Scale down slightly on active (0.97), spring back on release

### 3.6 Notification System

Replace the single-notice banner with a **toast stack**:

```
Position:        top-right, stacked vertically
Max visible:     4 toasts
Auto-dismiss:    info=4s, error=8s, critical=manual dismiss
Structure:       [icon] [message] [dismiss X]
                 [progress bar for auto-dismiss countdown]
Severity colors: info=blue-muted, error=danger-muted, critical=danger-bg
```

Add a small notification history dropdown accessible from the topbar.

### 3.7 Empty States

Current empty states are plain text: "No events captured yet." "Select a card to edit and run." "No active leases."

These should be **illustrated empty states** with:
- A relevant icon or simple SVG illustration
- Primary message (what's empty)
- Secondary message (what to do about it)
- Optional CTA button

Example for empty event stream:
```
     ⚡ (pulse icon)
  No events captured yet
  Connect to a gateway to see
  real-time events flow here.
  [Save + Connect]
```

### 3.8 Responsive & Density Considerations

The current responsive breakpoints (1160px, 760px) simply stack columns. For a Tauri desktop app, also consider:

- **Compact density mode**: Toggle between "comfortable" and "compact" spacing for operators who want maximum data density (think Bloomberg vs. casual browsing)
- **Panel collapsibility**: Allow the board drawer, mail compose panel, and cockpit sidebar to collapse to a narrow rail
- **Keyboard-first navigation**: This is a power-user tool. Every action should have a keyboard shortcut. Consider a command palette (Cmd+K)

---

## 4. Implementation Recommendations

### 4.1 CSS Architecture

When you modularize App.tsx (which you mentioned), also break up `styles.css`:

```
styles/
  tokens.css          /* CSS custom properties: colors, type, spacing, motion */
  reset.css           /* box-sizing, body, base element styles */
  layout.css          /* shell, grids, responsive breakpoints */
  components/
    topbar.css
    tabs.css
    surface.css       /* the generic panel/card pattern */
    chip.css
    button.css
    input.css
    notice.css        /* → toast.css */
    board.css
    calendar.css
    focus.css
    events.css
    mail.css
    chatroom.css
    cockpit.css
```

Or use CSS modules per component when modularizing.

### 4.2 Dark/Light Toggle

```css
:root { /* dark mode defaults */ }

:root[data-theme="light"] {
  /* light overrides */
}

/* Respect system preference as initial value */
@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) {
    /* light overrides */
  }
}
```

Store preference in localStorage, default to system preference.

### 4.3 Icon System

For a Tauri app, inline SVG icons are ideal (no network requests, tree-shakeable). Good options:
- **Lucide** (MIT, consistent, extensive) — the successor to Feather Icons
- **Phosphor** (MIT, multiple weights including fill/duotone)
- **Tabler Icons** (MIT, large set, clean style)

Pick one family and stick with it.

### 4.4 Priority Sequence for Implementation

If implementing this design pass incrementally:

```
Phase 0: DARK MODE + COLOR TOKENS  (highest impact, unblocks everything)
  → Define CSS custom properties for dark theme
  → Swap all hardcoded colors to variables
  → Add data-theme toggle + system preference detection

Phase 1: TYPOGRAPHY + SPACING
  → Import chosen fonts (Geist or Plus Jakarta Sans + JetBrains Mono)
  → Apply type scale tokens
  → Add monospace treatment to data values

Phase 2: NAVIGATION + TOPBAR
  → Redesign tab bar (icons + labels, active state, alert badges)
  → Redesign pinned health strip with semantic coloring
  → Add theme toggle to topbar

Phase 3: NOTIFICATION SYSTEM
  → Replace single notice with toast stack
  → Add auto-dismiss + severity-based duration

Phase 4: COMPONENT POLISH (per-view)
  → Board cards: color bars, tag chips, better metadata layout
  → Events: structured rendering, domain color-coding
  → Mail: sender avatars, message bubble styling
  → Cockpit: widget-type visual identity, edit mode toggle

Phase 5: MOTION
  → Tab transitions
  → Event stream animations
  → Toast enter/exit
  → Drag ghost styling
  → Button micro-interactions

Phase 6: POWER-USER FEATURES
  → Command palette (Cmd+K)
  → Keyboard shortcuts for tab switching
  → Compact density toggle
  → Collapsible panels
```

---

## 5. Visual Identity Quick Reference

When this design is implemented, Mission Control should feel like:

| Attribute | Current | Target |
|-----------|---------|--------|
| Background | Warm cream #f1ede6 | Deep charcoal #101114 |
| Surfaces | Near-white #fffdfa | Raised dark #1e2028 |
| Text | Dark brown #1f2022 | Warm light #e8e6e1 |
| Accent | Orange #ff6a13 | Same orange, now glowing |
| Borders | Visible on everything | Subtle, only for separation |
| Typography | IBM Plex Sans everywhere | Display + Body + Monospace triad |
| Motion | None | Purposeful, 120-400ms |
| Density | Airy | Operator-dense, configurable |
| Empty states | Plain text | Illustrated + actionable |
| Notifications | Single banner | Toast stack, auto-dismiss |
| Icons | None | Full icon system (Lucide/Phosphor) |
| Theme | Light only | Dark primary, light optional |

---

## 6. References & Inspiration

If you want to see what "this but good" looks like, study:

- **Vercel Dashboard** — dark mode ops done right, clean density
- **Linear** — buttery animations, keyboard-first, gorgeous dark theme
- **Grafana** — dense data dashboard with personality
- **Railway.app** — modern dark ops console
- **Raycast** — command palette UX, snappy interactions
- **Warp terminal** — how to make a power tool feel premium
- **Bloomberg Terminal** — the gold standard for "dense data, operator-first"

The goal isn't to copy any of these. It's to match their **intentionality** — where every pixel choice serves the operator's speed and awareness.

---

*This document is a design audit and proposal. No code was changed. Ready for discussion and implementation planning.*

---
---

# ADDENDUM: Extended Audit + Theme System

**Added:** 2026-02-27 (same session, second pass)

---

## 7. Additional UI/UX Issues Found in Deep Read

These go beyond visual aesthetics — they're interaction design, information architecture, and usability problems baked into the current implementation.

### 7.1 No URL Routing or Deep Links

The app uses `useState<MissionControlTab>("boards")` for navigation. There is no router. This means:

- **No deep linking** — you can't share a URL to a specific board, mail thread, or cockpit page
- **No browser back/forward** — tab switches are unrecoverable
- **No bookmarking** — every launch starts at "boards"
- **Page refresh loses all state** — except cockpit layout (localStorage) and connection settings

For a Tauri app this is somewhat mitigated (it's not a browser), but even Tauri supports navigation state. When modularizing, consider adding a lightweight router or at least hash-based routing (`#/boards`, `#/mail/thread-id`).

### 7.2 Connection Config Eats Prime Real Estate

The gateway URL + token fields + 3 buttons are **permanently visible** at the top of every view (lines 2539-2568). Once you're connected, you never touch these again until something breaks.

**Fix:** Collapse to a single status indicator in the topbar. Click to expand a settings panel/modal. First-launch shows the connection form prominently; subsequent launches auto-connect and hide it.

### 7.3 Zero Loading/Pending States

When you click "Save + Connect," "Run Card," "Send," "Approve," or any async action:
- The button doesn't disable
- No spinner appears
- No skeleton screens during data load
- The only feedback is a notice appearing some time later

This creates uncertainty: "Did I click it? Is it working? Should I click again?" Double-clicks on actions like "Approve" or "Run Card" could cause real problems.

**Fix:** Every async action needs: (1) immediate button disable + spinner, (2) optimistic state update where safe, (3) success/error feedback via toast.

### 7.4 The "Calendar" Has No Calendar

The Calendar tab (lines 2861-2975) contains:
- A "Week Planning" panel with two list lanes (Always Running / Next Up)
- A "Scheduler Matrix" table

Despite being called "Calendar," there is **no temporal visualization** — no week grid, no day columns, no timeline, no Gantt-style bars, no time-based positioning. Jobs with `next_run_at` timestamps are just listed in a table. You cannot visually see "what runs when" across a day or week.

**Fix:** Add an actual calendar/timeline component. Even a simple horizontal timeline showing job execution windows across the week would transform this tab's utility. The data is already there (`week_start_ms`, `week_end_ms`, `next_run_at`, `interval_seconds`).

### 7.5 No Confirmation on Destructive Actions

These actions execute immediately with no confirmation:
- "Clear Token" (line 2564) — disconnects you from the gateway
- Cockpit widget "Remove" (line 3725) — deletes a widget
- "Restore Defaults" cockpit (line 3677) — nukes your entire cockpit layout
- File lease "Release" (various) — releases an advisory lock
- Job "Pause" — stops a running scheduled job

For an ops tool where someone might have a chatroom lease protecting a multi-agent workflow, one misclick on "Release" could cause coordination failures.

**Fix:** At minimum, destructive actions should require either: a confirmation modal, or a 3-second undo toast ("Lease released. [Undo]").

### 7.6 Approval Actions Show No Context

In the Focus queue, approval items show a title and detail string, then "Approve" and "Deny" buttons. But there's **no way to inspect what you're approving** — the tool call arguments, the command to be executed, the file to be written.

The `action_payload` object is available (`item.action_payload.approval_id`) but none of the approval's `request_summary` or `request_json` is fetched or displayed.

**Fix:** Each approval item should have an expandable detail section showing the full request: tool name, arguments, requesting agent, target session. The operator should never have to approve blind.

### 7.7 Chatroom Reactions Are Fake

Lines 3449-3457:
```tsx
<button onClick={() => void postRoomReaction(":+1:")}>+1</button>
<button onClick={() => void postRoomReaction(":eyes:")}>eyes</button>
<button onClick={() => void postRoomReaction(":white_check_mark:")}>done</button>
```

These send a literal message `"reaction :+1:"` as a new message in the thread. They're not reactions in any meaningful sense — they pollute the message stream with noise, and there's no visual treatment to distinguish "reaction messages" from real messages.

**Fix:** Either implement proper reactions as metadata on messages (server-side support needed), or at minimum visually differentiate reaction messages in the stream (compact inline chips rather than full message bubbles). Also: add an emoji picker rather than hardcoding three options.

### 7.8 Lease TTL is Raw Milliseconds

```tsx
<input value={leaseTtlMs} placeholder="ttl ms" />
```

The user has to type `900000` for 15 minutes, `3600000` for 1 hour. No human thinks in milliseconds.

**Fix:** Either a duration picker (number + unit dropdown: seconds/minutes/hours), or preset buttons (15m, 1h, 4h, 24h) with a custom option.

### 7.9 No Relative Timestamps

Every timestamp in the app uses:
```tsx
function formatDateTime(unixMs: number | null | undefined): string {
  return new Date(unixMs).toLocaleString();
}
```

This gives you `2/27/2026, 3:42:15 PM` everywhere. In an ops context, relative times are far more scannable: "2m ago", "just now", "yesterday at 3:42 PM." The absolute timestamp can be shown on hover as a tooltip.

### 7.10 No Search or Filter on Boards

You can select a board from a dropdown, but within a board there's no way to:
- Search cards by title or description
- Filter by tag
- Filter by owner (agent/human/unassigned)
- Filter by column
- Sort cards differently

For boards with many cards across many columns, this makes finding specific work items a scroll-and-scan exercise.

### 7.11 Cockpit Widget Controls Are Always Visible

Every cockpit widget header shows 5 buttons at all times: Up, Down, -, +, Remove. With 9 widgets on the default page, that's 45 control buttons visible simultaneously, creating massive visual noise.

**Fix:** Add a "Edit Layout" toggle. In view mode, widget headers show just the title. In edit mode, controls appear with a distinct visual treatment (dashed borders, grab handles, etc). This is how Grafana, Notion, and every other dashboard builder works.

### 7.12 No Accessibility Layer

Zero `aria-*` attributes in 3,745 lines. No `role` annotations. No focus management after async operations. No skip-navigation. Screen readers would have no idea what's happening. The drag-and-drop on boards uses native HTML drag events with no keyboard alternative.

This matters less for a personal Tauri desktop app but should be on the radar for any future multi-user scenario.

### 7.13 Mail Compose Drafts Don't Persist

If you're composing a mail message and switch to the Boards tab, your draft text, recipients, and attachments are gone when you switch back. Each tab renders/unmounts conditionally:

```tsx
{activeTab === "mail" ? ( ... ) : null}
```

**Fix:** Either keep all tab content mounted (hidden via CSS) so React state persists, or save drafts to a ref/context that survives tab switches.

### 7.14 Board Column Width is Hardcoded

```css
.mc-board-column-wrap {
  width: 308px;
}
```

No user preference, no responsive sizing, no way to have wider columns for boards with longer card titles. The virtualizer's `estimateSize: () => 320` is also hardcoded to match.

### 7.15 No Keyboard Shortcuts

For a power-user ops tool, this is the single biggest productivity gap. There are zero keyboard shortcuts in the app. Recommended:

```
1-7              Switch tabs
Cmd+K            Command palette / global search
Cmd+Enter        Submit current form (send message, save card, etc.)
Escape           Close drawer / deselect card / dismiss modal
Cmd+Shift+I      Toggle incident mode
Cmd+R            Refresh current view data
J/K              Navigate focus items / mail threads
A/D              Approve/Deny focused approval item
```

---

## 8. Theme System: 5 Distinct Identities

Each theme includes **both dark and light variants** and is designed as a complete aesthetic identity — not just a color swap. Different fonts, different spatial feels, different moods.

The theme system should be implemented as CSS custom property sets. Switching themes swaps the variable set on `:root`. All component CSS references variables only, never hardcoded colors.

```
Theme structure:
  :root[data-theme="obsidian-dark"]   { ... }
  :root[data-theme="obsidian-light"]  { ... }
  :root[data-theme="phosphor-dark"]   { ... }
  :root[data-theme="phosphor-light"]  { ... }
  ... etc
```

---

### Theme 1: "Obsidian Ops" (The Default)

**Personality:** Professional. Precise. Modern dark ops console. The Vercel/Linear school of design — where restraint IS the flex. Everything is deliberate. Nothing is decorative. You open this and immediately feel like you're in control of something important.

**Who it's for:** The default experience. Operators who want a clean, focused, no-BS tool.

**What makes it unforgettable:** The way the orange accent *glows* against the dark canvas. Active elements feel like they're lit from within. The contrast between the stark dark environment and the warm amber signals creates an instinctive hierarchy — your eye always goes where the light is.

#### Dark Variant (Primary)

```css
/* === OBSIDIAN OPS — DARK === */

/* Fonts */
--font-display: "Unbounded", sans-serif;
--font-body: "Geist", "Plus Jakarta Sans", system-ui, sans-serif;
--font-mono: "Geist Mono", "JetBrains Mono", "Berkeley Mono", monospace;

/* Background layers */
--bg-void: #08080a;
--bg: #0e1012;
--bg-raised: #151719;
--bg-overlay: #1a1d21;

/* Surfaces */
--surface: #1c1f25;
--surface-hover: #23272e;
--surface-active: #2a2f38;

/* Borders */
--line: #252a33;
--line-strong: #353b48;
--line-focus: rgba(255, 106, 19, 0.4);

/* Text */
--ink-primary: #eae8e3;
--ink-secondary: #858892;
--ink-muted: #4e525c;
--ink-inverse: #0e1012;

/* Accent — the signature amber */
--accent: #ff6a13;
--accent-hover: #ff8540;
--accent-glow: rgba(255, 106, 19, 0.12);
--accent-glow-strong: rgba(255, 106, 19, 0.25);
--accent-ink: #fff0e2;
--accent-surface: #2a1708;

/* Semantic */
--ok: #22c55e;
--ok-surface: rgba(34, 197, 94, 0.08);
--ok-border: rgba(34, 197, 94, 0.2);
--warn: #eab308;
--warn-surface: rgba(234, 179, 8, 0.08);
--warn-border: rgba(234, 179, 8, 0.2);
--danger: #ef4444;
--danger-surface: rgba(239, 68, 68, 0.08);
--danger-border: rgba(239, 68, 68, 0.25);
--info: #3b82f6;
--info-surface: rgba(59, 130, 246, 0.08);
--info-border: rgba(59, 130, 246, 0.2);

/* Shadows — subtle, layered */
--shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
--shadow-md: 0 4px 16px rgba(0, 0, 0, 0.35);
--shadow-lg: 0 12px 40px rgba(0, 0, 0, 0.45);
--shadow-glow: 0 0 20px var(--accent-glow);

/* Radius */
--radius-sm: 6px;
--radius-md: 10px;
--radius-lg: 14px;
--radius-full: 999px;
```

#### Light Variant

```css
/* === OBSIDIAN OPS — LIGHT === */
/* Cool-gray foundation, NOT warm beige. Crisp. */

--bg-void: #edeef1;
--bg: #f4f5f7;
--bg-raised: #ffffff;
--bg-overlay: #ffffff;

--surface: #ffffff;
--surface-hover: #f0f1f4;
--surface-active: #e6e8ed;

--line: #dcdfe5;
--line-strong: #c5c9d2;
--line-focus: rgba(255, 106, 19, 0.4);

--ink-primary: #0c0e13;
--ink-secondary: #5f6470;
--ink-muted: #9ca1ac;
--ink-inverse: #ffffff;

--accent: #e55a0a;      /* slightly deeper for light bg contrast */
--accent-hover: #ff6a13;
--accent-glow: rgba(229, 90, 10, 0.08);
--accent-glow-strong: rgba(229, 90, 10, 0.15);
--accent-ink: #ffffff;
--accent-surface: #fff4ec;

--ok: #16a34a;
--ok-surface: #edfcf2;
--warn: #ca8a04;
--warn-surface: #fefce8;
--danger: #dc2626;
--danger-surface: #fef2f2;
--info: #2563eb;
--info-surface: #eff6ff;

--shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
--shadow-md: 0 4px 12px rgba(0, 0, 0, 0.07);
--shadow-lg: 0 8px 30px rgba(0, 0, 0, 0.1);
--shadow-glow: 0 0 16px rgba(229, 90, 10, 0.1);
```

---

### Theme 2: "Phosphor"

**Personality:** Retro-terminal. The ghost of CRT monitors past. Green-on-black (or amber-on-black). Monospace everything. Subtle scanline texture. This is for the operator who misses the feeling of `htop` running in a dark room at 2 AM.

**Who it's for:** The hacker, the sysadmin at heart, the person who thinks GUIs peaked with ncurses. They run this tool and feel like they're SSH'd into the future.

**What makes it unforgettable:** The scanline overlay. The CRT bloom effect on active elements. Text that feels like it's being rendered by a phosphor beam. Everything is monospace. The whole app feels like a single terminal session that got really, really good at its job.

#### Dark Variant (Primary)

```css
/* === PHOSPHOR — DARK === */

--font-display: "Departure Mono", "VT323", monospace;
/* NOTE: Departure Mono is a beautiful pixel/terminal display font.
   Fallback to VT323 (Google Fonts, free).
   If neither available, "IBM Plex Mono" as safe fallback. */
--font-body: "IBM Plex Mono", "Fira Code", monospace;
--font-mono: "IBM Plex Mono", "Fira Code", monospace;
/* YES: body and mono are the same. Everything is monospace. That's the point. */

--bg-void: #000000;
--bg: #0a0d08;
--bg-raised: #0f130c;
--bg-overlay: #141a10;

--surface: #111710;
--surface-hover: #1a2216;
--surface-active: #222d1d;

--line: #1e2b18;
--line-strong: #2d4023;
--line-focus: rgba(57, 255, 20, 0.35);

--ink-primary: #b8e6a0;     /* soft green, high readability */
--ink-secondary: #6b9955;
--ink-muted: #3d5c2e;
--ink-inverse: #000000;

/* Accent — phosphor green */
--accent: #39ff14;
--accent-hover: #5fff3f;
--accent-glow: rgba(57, 255, 20, 0.1);
--accent-glow-strong: rgba(57, 255, 20, 0.25);
--accent-ink: #000000;
--accent-surface: #0a1a06;

/* Semantic — all in the green/amber/red family to stay in palette */
--ok: #39ff14;
--ok-surface: rgba(57, 255, 20, 0.06);
--ok-border: rgba(57, 255, 20, 0.2);
--warn: #ffb627;         /* amber phosphor */
--warn-surface: rgba(255, 182, 39, 0.06);
--warn-border: rgba(255, 182, 39, 0.2);
--danger: #ff3333;
--danger-surface: rgba(255, 51, 51, 0.06);
--danger-border: rgba(255, 51, 51, 0.25);
--info: #39ff14;         /* info = accent in this theme */
--info-surface: rgba(57, 255, 20, 0.06);
--info-border: rgba(57, 255, 20, 0.15);

--shadow-sm: none;
--shadow-md: 0 0 8px rgba(57, 255, 20, 0.05);
--shadow-lg: 0 0 24px rgba(57, 255, 20, 0.08);
--shadow-glow: 0 0 12px rgba(57, 255, 20, 0.2);

--radius-sm: 2px;       /* terminal aesthetic = sharp corners */
--radius-md: 3px;
--radius-lg: 4px;
--radius-full: 999px;

/* SPECIAL: Phosphor extras */
--scanline-opacity: 0.03;
--text-shadow-glow: 0 0 6px rgba(57, 255, 20, 0.3);
--crt-bloom: 0 0 40px rgba(57, 255, 20, 0.04);
```

**Special CSS for Phosphor theme:**
```css
/* CRT scanline overlay — applied to body::after */
[data-theme^="phosphor"] body::after {
  content: "";
  position: fixed;
  inset: 0;
  pointer-events: none;
  z-index: 9999;
  background: repeating-linear-gradient(
    transparent 0px,
    transparent 2px,
    rgba(0, 0, 0, var(--scanline-opacity)) 2px,
    rgba(0, 0, 0, var(--scanline-opacity)) 4px
  );
}

/* Text glow on primary content */
[data-theme^="phosphor"] .mc-brand-block h1,
[data-theme^="phosphor"] .mc-surface-header h2 {
  text-shadow: var(--text-shadow-glow);
}
```

#### Light Variant (The "Printout")

```css
/* === PHOSPHOR — LIGHT === */
/* Like a greenbar continuous-feed printout. */

--font-display: "Departure Mono", "VT323", monospace;
--font-body: "IBM Plex Mono", "Fira Code", monospace;
--font-mono: "IBM Plex Mono", "Fira Code", monospace;

--bg-void: #e8ede4;
--bg: #f0f4ec;
--bg-raised: #f8faf6;
--bg-overlay: #ffffff;

--surface: #f5f8f2;
--surface-hover: #e8ede3;
--surface-active: #dce4d5;

--line: #c4d4b8;
--line-strong: #a4ba92;

--ink-primary: #1a2e10;
--ink-secondary: #4a6638;
--ink-muted: #8aaa76;

--accent: #1a8c09;
--accent-hover: #22a310;
--accent-glow: rgba(26, 140, 9, 0.08);

/* Greenbar stripe effect for table rows */
--greenbar-stripe: rgba(200, 230, 185, 0.3);

--scanline-opacity: 0.015;   /* very subtle on light */
```

---

### Theme 3: "Arctic"

**Personality:** Cold. Clean. Scandinavian-minimal. Ice-blue accents on vast white/silver expanses. Razor-thin typography. Generous negative space. This is Dieter Rams designing an AI ops console in a Norwegian fjord cabin. Less is more. Way more.

**Who it's for:** The person who finds beauty in restraint. Who thinks Notion is too cluttered. Who wants their ops tool to feel like a high-end architectural rendering.

**What makes it unforgettable:** The *silence* of it. Massive whitespace. Ultra-thin hairline borders. Type so precise it looks laser-cut. Ice-blue accents that feel like bioluminescence. When something demands attention (a critical alert), the visual disruption against all that calm is genuinely startling.

#### Dark Variant

```css
/* === ARCTIC — DARK === */

--font-display: "Outfit", sans-serif;
/* Outfit: geometric, clean, very Scandinavian. Multiple weights. Google Fonts. */
--font-body: "Outfit", sans-serif;
--font-mono: "Berkeley Mono", "Fira Code", monospace;

--bg-void: #090b10;
--bg: #0c0f16;
--bg-raised: #11141c;
--bg-overlay: #161a24;

--surface: #141824;
--surface-hover: #1a1f2d;
--surface-active: #222838;

--line: #1e2436;
--line-strong: #2a3248;
--line-focus: rgba(96, 180, 255, 0.35);

--ink-primary: #dfe3ea;
--ink-secondary: #7a8396;
--ink-muted: #454d60;
--ink-inverse: #0c0f16;

/* Accent — glacial blue */
--accent: #60b4ff;
--accent-hover: #80c6ff;
--accent-glow: rgba(96, 180, 255, 0.1);
--accent-glow-strong: rgba(96, 180, 255, 0.22);
--accent-ink: #0c0f16;
--accent-surface: #0d1a2e;

/* Semantic */
--ok: #34d399;
--ok-surface: rgba(52, 211, 153, 0.07);
--ok-border: rgba(52, 211, 153, 0.18);
--warn: #fbbf24;
--warn-surface: rgba(251, 191, 36, 0.07);
--warn-border: rgba(251, 191, 36, 0.18);
--danger: #f87171;
--danger-surface: rgba(248, 113, 113, 0.07);
--danger-border: rgba(248, 113, 113, 0.22);
--info: #60b4ff;
--info-surface: rgba(96, 180, 255, 0.07);
--info-border: rgba(96, 180, 255, 0.18);

--shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.2);
--shadow-md: 0 4px 20px rgba(0, 0, 0, 0.25);
--shadow-lg: 0 12px 48px rgba(0, 0, 0, 0.35);
--shadow-glow: 0 0 24px rgba(96, 180, 255, 0.08);

--radius-sm: 8px;
--radius-md: 12px;
--radius-lg: 18px;
--radius-full: 999px;

/* SPECIAL: Arctic uses hairline borders (0.5px on retina) */
--border-width: 0.5px;
```

#### Light Variant

```css
/* === ARCTIC — LIGHT === */
/* Snow and ice. Vast whitespace. Blue shadows. */

--bg-void: #eef2f7;
--bg: #f5f7fb;
--bg-raised: #ffffff;
--bg-overlay: #ffffff;

--surface: #ffffff;
--surface-hover: #f0f3f9;
--surface-active: #e4e9f2;

--line: #dfe4ed;
--line-strong: #c7ced9;

--ink-primary: #0a0f1a;
--ink-secondary: #5a6478;
--ink-muted: #a0a8b8;

--accent: #2b7de9;
--accent-hover: #4090f0;
--accent-glow: rgba(43, 125, 233, 0.06);
--accent-ink: #ffffff;
--accent-surface: #eef4fd;

/* Shadows with blue tint — the Arctic signature */
--shadow-sm: 0 1px 3px rgba(43, 80, 160, 0.06);
--shadow-md: 0 4px 16px rgba(43, 80, 160, 0.08);
--shadow-lg: 0 12px 40px rgba(43, 80, 160, 0.1);
--shadow-glow: 0 0 20px rgba(43, 125, 233, 0.08);
```

---

### Theme 4: "Midnight Ember"

**Personality:** Rich. Warm. Premium. This is the whiskey-by-the-fireplace of ops consoles. Deep navy and charcoal backgrounds with copper and rose-gold accents. Slightly rounded, slightly soft. Think: if a luxury watch brand made a dashboard app. Not cold, not clinical — *warm and confident*.

**Who it's for:** The operator who wants their tool to feel premium rather than utilitarian. Who appreciates craft. Who'd pay extra for the leather-band Apple Watch.

**What makes it unforgettable:** The copper/rose-gold accent is genuinely unusual in tech tools. Combined with deep warm darks and slightly serif-tinged display type, it creates something that feels bespoke. Like a custom-built instrument panel in a hand-crafted car.

#### Dark Variant (Primary)

```css
/* === MIDNIGHT EMBER — DARK === */

--font-display: "DM Serif Text", "Playfair Display", serif;
/* DM Serif Text: elegant, readable display serif. Google Fonts. */
--font-body: "DM Sans", "Nunito Sans", sans-serif;
/* DM Sans: the sans companion to DM Serif. Clean, slightly warm. */
--font-mono: "JetBrains Mono", "Fira Code", monospace;

--bg-void: #0b0a0e;
--bg: #100f14;
--bg-raised: #17151c;
--bg-overlay: #1d1b24;

--surface: #1a1822;
--surface-hover: #22202c;
--surface-active: #2c2936;

--line: #262332;
--line-strong: #383446;
--line-focus: rgba(224, 160, 110, 0.35);

--ink-primary: #ece5dc;
--ink-secondary: #8e8494;
--ink-muted: #55505e;
--ink-inverse: #100f14;

/* Accent — copper / rose gold */
--accent: #e0a06e;
--accent-hover: #ebba90;
--accent-glow: rgba(224, 160, 110, 0.1);
--accent-glow-strong: rgba(224, 160, 110, 0.22);
--accent-ink: #1a1008;
--accent-surface: #1f1710;

/* Semantic — warmer versions */
--ok: #6ee0a0;
--ok-surface: rgba(110, 224, 160, 0.07);
--ok-border: rgba(110, 224, 160, 0.18);
--warn: #e0c56e;
--warn-surface: rgba(224, 197, 110, 0.07);
--warn-border: rgba(224, 197, 110, 0.18);
--danger: #e07070;
--danger-surface: rgba(224, 112, 112, 0.07);
--danger-border: rgba(224, 112, 112, 0.22);
--info: #7098e0;
--info-surface: rgba(112, 152, 224, 0.07);
--info-border: rgba(112, 152, 224, 0.18);

--shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.25);
--shadow-md: 0 6px 20px rgba(0, 0, 0, 0.3);
--shadow-lg: 0 16px 48px rgba(0, 0, 0, 0.4);
--shadow-glow: 0 0 24px rgba(224, 160, 110, 0.08);

--radius-sm: 8px;
--radius-md: 12px;
--radius-lg: 16px;
--radius-full: 999px;
```

#### Light Variant

```css
/* === MIDNIGHT EMBER — LIGHT === */
/* Warm parchment but REFINED, not the current beige soup. */

--bg-void: #f0ebe4;
--bg: #f6f1ea;
--bg-raised: #fdfaf6;
--bg-overlay: #ffffff;

--surface: #fdfaf6;
--surface-hover: #f3ece3;
--surface-active: #e8dfd3;

--line: #ddd3c5;
--line-strong: #c8bba8;

--ink-primary: #1a1510;
--ink-secondary: #6e6358;
--ink-muted: #a89d90;

--accent: #b87530;
--accent-hover: #cc8840;
--accent-glow: rgba(184, 117, 48, 0.08);
--accent-ink: #ffffff;
--accent-surface: #fdf4eb;

/* This light variant is actually closest to the CURRENT design
   but with proper contrast ratios and intentional hierarchy */
--shadow-sm: 0 1px 3px rgba(80, 60, 30, 0.06);
--shadow-md: 0 4px 16px rgba(80, 60, 30, 0.08);
--shadow-lg: 0 12px 40px rgba(80, 60, 30, 0.1);
```

---

### Theme 5: "Brutalist"

**Personality:** Raw. Uncompromising. High-contrast. This is anti-design as design. Black and white with a single screaming accent color. No rounded corners. Thick borders. System fonts or raw monospace. Dense. Aggressive. Every element announces itself. No shadows — shadows are for cowards. This is a concrete bunker with fluorescent lighting and it does NOT care about your feelings.

**Who it's for:** The person who thinks most software is drowning in unnecessary polish. Who wants data density above all else. Who'd run their ops console in a terminal if the terminal could do kanban. The anti-aesthetic IS the aesthetic.

**What makes it unforgettable:** It's *jarring* in the best way. In a world of soft corners and gentle gradients, this thing hits like a concrete wall. The single accent color (electric red or hot yellow) against pure black/white creates an urgency that never lets you relax. Which, for an ops console managing autonomous AI agents, might be exactly right.

#### Dark Variant

```css
/* === BRUTALIST — DARK === */

--font-display: "Obviously", "Archivo Black", "Impact", sans-serif;
/* Obviously: bold, wide, industrial. If unavailable:
   Archivo Black (Google Fonts) as fallback. */
--font-body: "Archivo", "Helvetica Neue", system-ui, sans-serif;
/* Archivo: clean, slightly condensed, industrial feel. Google Fonts. */
--font-mono: "IBM Plex Mono", "Courier New", monospace;

--bg-void: #000000;
--bg: #000000;
--bg-raised: #0a0a0a;
--bg-overlay: #111111;

--surface: #0a0a0a;
--surface-hover: #1a1a1a;
--surface-active: #252525;

/* Borders — THICK, visible, intentional */
--line: #333333;
--line-strong: #555555;
--line-focus: var(--accent);

--ink-primary: #ffffff;
--ink-secondary: #999999;
--ink-muted: #555555;
--ink-inverse: #000000;

/* Accent — electric red. One color. That's it. */
--accent: #ff2020;
--accent-hover: #ff4545;
--accent-glow: rgba(255, 32, 32, 0.15);
--accent-glow-strong: rgba(255, 32, 32, 0.3);
--accent-ink: #ffffff;
--accent-surface: #1a0000;

/* Semantic — minimal palette */
--ok: #00ff00;          /* pure green. no subtlety. */
--ok-surface: rgba(0, 255, 0, 0.05);
--ok-border: #00ff00;
--warn: #ffff00;        /* pure yellow */
--warn-surface: rgba(255, 255, 0, 0.05);
--warn-border: #ffff00;
--danger: #ff2020;
--danger-surface: rgba(255, 32, 32, 0.05);
--danger-border: #ff2020;
--info: #ffffff;
--info-surface: rgba(255, 255, 255, 0.05);
--info-border: #ffffff;

/* NO SHADOWS. Shadows are fake depth. Borders are honest. */
--shadow-sm: none;
--shadow-md: none;
--shadow-lg: none;
--shadow-glow: none;

/* NO RADIUS. Rectangles. */
--radius-sm: 0;
--radius-md: 0;
--radius-lg: 0;
--radius-full: 0;

/* SPECIAL: Brutalist border width */
--border-width: 2px;
```

**Special CSS for Brutalist:**
```css
[data-theme^="brutalist"] * {
  border-radius: 0 !important;
}

[data-theme^="brutalist"] button {
  text-transform: uppercase;
  letter-spacing: 0.1em;
  font-weight: 700;
  border-width: 2px;
}

[data-theme^="brutalist"] button:hover {
  transform: none;              /* no cute hover lifts */
  background: var(--accent);
  color: var(--accent-ink);
  border-color: var(--accent);
}

/* Labels are SCREAMING */
[data-theme^="brutalist"] .mc-surface-header h2 {
  text-transform: uppercase;
  letter-spacing: 0.08em;
  border-bottom: 2px solid var(--accent);
  padding-bottom: 0.3em;
  display: inline-block;
}
```

#### Light Variant

```css
/* === BRUTALIST — LIGHT === */
/* Newsprint. Black on white. Unforgiving. */

--bg-void: #ffffff;
--bg: #ffffff;
--bg-raised: #f5f5f5;
--bg-overlay: #ffffff;

--surface: #f5f5f5;
--surface-hover: #eeeeee;
--surface-active: #e0e0e0;

--line: #000000;         /* yes, pure black borders on white */
--line-strong: #000000;

--ink-primary: #000000;
--ink-secondary: #444444;
--ink-muted: #999999;

/* Accent shifts to hot yellow for visibility on white */
--accent: #ff2020;
--accent-hover: #cc0000;
--accent-glow: rgba(255, 32, 32, 0.08);
--accent-ink: #ffffff;
--accent-surface: #fff0f0;

/* Even bolder borders on light */
--border-width: 2px;
```

---

## 9. Theme Comparison At a Glance

| | Obsidian Ops | Phosphor | Arctic | Midnight Ember | Brutalist |
|---|---|---|---|---|---|
| **Mood** | Professional precision | Retro hacker | Scandinavian calm | Luxury warmth | Industrial raw |
| **Display Font** | Unbounded | Departure Mono | Outfit | DM Serif Text | Archivo Black |
| **Body Font** | Geist | IBM Plex Mono | Outfit | DM Sans | Archivo |
| **Dark Accent** | #ff6a13 amber | #39ff14 green | #60b4ff ice-blue | #e0a06e copper | #ff2020 red |
| **Light Accent** | #e55a0a | #1a8c09 | #2b7de9 | #b87530 | #ff2020 |
| **Border Radius** | 6-14px | 2-4px | 8-18px | 8-16px | 0px |
| **Shadows** | Subtle layered | Glow only | Blue-tinted | Deep warm | None |
| **Special FX** | Accent glow | Scanlines, CRT bloom | Hairline borders | — | Thick borders |
| **All Monospace?** | No | YES | No | No | No |
| **Density** | Medium | High | Low (spacious) | Medium | Very high |
| **Best For** | Daily ops | Night owl hackers | Design-conscious | Executive vibe | Data maximalists |

---

## 10. Implementation Notes for Theme System

### Switching Mechanism

```typescript
type ThemeName =
  | "obsidian-dark" | "obsidian-light"
  | "phosphor-dark" | "phosphor-light"
  | "arctic-dark"   | "arctic-light"
  | "ember-dark"    | "ember-light"
  | "brutalist-dark" | "brutalist-light";

function applyTheme(theme: ThemeName): void {
  document.documentElement.setAttribute("data-theme", theme);
  localStorage.setItem("mission_control.theme", theme);
}

function getInitialTheme(): ThemeName {
  const stored = localStorage.getItem("mission_control.theme") as ThemeName | null;
  if (stored) return stored;
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  return prefersDark ? "obsidian-dark" : "obsidian-light";
}
```

### CSS File Structure

```
styles/
  themes/
    _tokens-shared.css       /* spacing, type scale, motion tokens — shared by all */
    obsidian-dark.css
    obsidian-light.css
    phosphor-dark.css
    phosphor-light.css
    arctic-dark.css
    arctic-light.css
    ember-dark.css
    ember-light.css
    brutalist-dark.css
    brutalist-light.css
    brutalist-overrides.css  /* the !important stuff for radius/borders */
    phosphor-overrides.css   /* scanlines, CRT bloom */
```

### Font Loading Strategy

Each theme declares its own Google Fonts import. Only load fonts for the active theme:

```typescript
const THEME_FONTS: Record<string, string> = {
  obsidian: "Unbounded:wght@500;700&family=Geist:wght@400;500;600",
  phosphor: "IBM+Plex+Mono:wght@400;500;600&family=VT323",
  arctic:   "Outfit:wght@300;400;500;600;700",
  ember:    "DM+Serif+Text&family=DM+Sans:wght@400;500;600",
  brutalist:"Archivo:wght@400;600;700&family=Archivo+Black",
};
```

Dynamically inject/swap the `<link>` tag when theme changes.

---

*End of addendum. Total design pass document now covers: full audit, design direction, extended UX issues, and 5 complete theme systems with dark + light variants each.*

---
---

# ADDENDUM 2: Operator UX Mandates + New Pages

**Added:** 2026-02-28
**Context:** Operator feedback session. These requirements are non-negotiable and override any conflicting recommendation in Sections 1–10.

---

## 11. THE LAWS (Non-Negotiable)

These override every other design decision in this document. If a recommendation above conflicts with a Law, the Law wins.

### Law 1: ZERO SCROLL BARS

No page, panel, or view may produce a visible scroll bar. Ever. Content that would overflow must be handled through:

- **Tabs / sub-tabs** within the feature page
- **Pagination** (numbered or load-more) for lists
- **Collapsible sections** for secondary content
- **Modals / drawers** for creation and editing flows
- **Dropdown menus** for options that would otherwise stack vertically
- **Fixed-height containers** that show the most recent/relevant N items

The *only* exception is the Kanban board horizontal axis (spatial panning, not scrolling through content).

Scroll bars are forbidden in: thread lists, message streams, event logs, focus queues, widget palettes, drawer forms, calendar tables, lease panels, asset lists, cockpit canvases.

**Current violations:** 17 scrollable regions across the app. Target: 0.

| Page | Scroll Regions | Fix |
|------|---------------|-----|
| Boards | 3 (canvas, drawer, assets) | Paginate columns, modal for card editor |
| Calendar | 3 (2 lanes, table) | Sub-tabs + paginated table |
| Focus | 1 (focus list) | Paginate |
| Events | 1 (events container) | Fixed-height, "Load more" button |
| Mail | 3 (threads, messages, leases) | Paginate all, move leases to sub-tab |
| Chatrooms | 3 (rooms, messages, moderation) | Paginate, moderation in modal |
| Cockpit | 3 (palette, canvas, widget bodies) | Palette→dropdown, fixed-height widgets |

### Law 2: CAVEMAN SIMPLICITY

If a page shows more than **5 interactive controls** at rest, it's too complex. Redesign with:

- **Progressive disclosure** — show primary actions, hide the rest behind "More" or a sub-tab
- **Modals for creation flows** — don't inline forms into already-busy panels
- **Sensible defaults** — pre-fill what can be pre-filled, auto-select what can be auto-selected
- **Context-sensitive controls** — only show actions relevant to the current state

Target: a new user with zero training should be able to use every feature within 30 seconds of seeing it.

**Current violations:**

| Page | Controls Visible | Fix |
|------|-----------------|-----|
| Boards (drawer) | 15 | Card editor → tabbed modal (Details/Script/Assets) |
| Mail | 17 | 2-col layout, inline compose, leases in sub-tab |
| Cockpit | 10+ | Widget palette→dropdown, edit mode toggle |
| Chatrooms | 8 | Moderation in modal |
| AppShell | 6 | Connection config in settings modal |

### Law 3: DROPDOWNS OVER TEXT INPUTS

Wherever the user is selecting from a known or enumerable set, use a dropdown/select — never a text field.

| Location | Current | Should Be | Data Source |
|----------|---------|-----------|-------------|
| Board card: Owner | text input | Dropdown | `listAgents()` |
| Board card: Tags | CSV text | Multi-select tag picker | Aggregate from loaded cards |
| Mail: Principal Override | text input | Dropdown | `listAgents()` + thread participants |
| Mail: Sender | text input | Dropdown | `listAgents()` |
| Mail: Recipients | CSV text | Multi-select | `listAgents()` + thread participants |
| Mail: Lease Holder | text input | Dropdown | `listAgents()` |
| Mail: Lease Glob | text input | Preset dropdown + custom | Hardcoded common patterns |
| Mail: Lease TTL | text "ttl ms" | Preset buttons (5m/15m/1h/4h/24h) | Frontend conversion to ms |
| Chatrooms: Sender | text input | Dropdown | `listAgents()` |
| Chatrooms: Recipients | CSV text | Multi-select | `listAgents()` |
| AppShell: Gateway URL | text input | Combo dropdown with history | localStorage |

Free-text inputs are only acceptable for: search queries, message body, card descriptions, card titles, thread subjects, and truly freeform content.

### Law 4: NOTHING BURIED

Every feature and action must be reachable in **2 clicks or fewer** from the main view:

```
Tab (1 click) → Sub-tab or action (1 click) → done
```

No hidden menus inside hidden menus. No settings pages inside drawers inside panels. If an operator needs something, it's *right there*.

---

## 12. Page-by-Page Zero-Scroll Architecture

### 12.1 App Shell

**Current:** Topbar + health strip + connection config (3 inputs, 3 buttons) + tab bar = 4 permanent sections eating vertical space.

**Redesign:**

```
┌─────────────────────────────────────────────────────┐
│ ☰ CarsinOS        [health chips]    [⚙] [◐] [●]   │  ← single topbar line
├──┬──────────────────────────────────────────────────┤
│  │                                                   │
│N │                                                   │
│A │          ACTIVE TAB CONTENT                       │
│V │          (fills all remaining space)              │
│  │                                                   │
├──┴──────────────────────────────────────────────────┤
```

- **Connection config** → **Settings gear icon [⚙]** in topbar. Opens a **modal** with Gateway URL (combo dropdown with history), Token, Connect/Clear. Auto-connect on launch. Connection status = colored dot [●] (green/amber/red). Click dot = open settings modal.
- **Health strip** → **Inline chips** in topbar. `Agents: 3  Jobs: 2  Approvals: 1  Channels: 4/4`. Click a chip = navigate to relevant tab.
- **Tab bar** → **Vertical nav rail** on left. Icon + short label. Active = accent left border. Alert badges (red dot on Focus when approvals pending).
- **Incident mode** → Toggle in topbar. Active = topbar border glows danger-red.

### 12.2 Boards

- **Column cards paginated** — N cards that fit viewport, page indicator per column. No vertical scroll.
- **"+ New Card"** → **creation modal** (Title, Column dropdown, Owner dropdown, Description). 3 fields + 2 buttons.
- **Click card** → **Card Detail Modal** with **sub-tabs**: `[Details] [Script] [Assets]`
  - Details: Title, Column (dropdown), Owner (dropdown), Due (picker), Tags (multi-select chip picker). Max 5 controls.
  - Script: Full-height markdown editor.
  - Assets: Upload + paginated grid.
- **Filter** → inline filter tabs at top: `[All] [agent-alpha] [agent-beta] [▼ By Tag]` (inspired by OpenClaw MC reference).
- **Per-page stats header**: `Cards: 12  In Progress: 3  Done: 7  This Week: +4`

### 12.3 Calendar

**Sub-tabs:** `[Week View] [Schedule] [Active Jobs]`

**Week View** (NEW — based on OpenClaw MC reference):
```
┌─ Always Running ─────────────────────────────────────┐
│ ⚡ mission-control-check • Every 30 min              │
└──────────────────────────────────────────────────────┘

┌─────┬─────┬─────┬─────┬─────┬─────┬─────┐
│ Sun │ Mon │ Tue │ Wed │ Thu │ Fri │ Sat │
├─────┼─────┼─────┼─────┼─────┼─────┼─────┤
│░░░░░│░░░░░│░░░░░│░░░░░│░░░░░│░░░░░│░░░░░│  ← color-coded
│ ai  │ ai  │ ai  │ ai  │ ai  │ ai  │ ai  │     job blocks
│ res │ res │ res │ res │ res │ res │ res │     positioned
│─────│─────│─────│─────│─────│─────│─────│     by time
│ brief│brief│brief│brief│brief│brief│brief│
│     │     │news │     │     │     │     │
└─────┴─────┴─────┴─────┴─────┴─────┴─────┘

┌─ Next Up ────────────────────────────────────────────┐
│ mission-control-check    in 12 min                   │
│ competitor-scan          in 1 hour                    │
│ morning-brief            in 20 hours                  │
└──────────────────────────────────────────────────────┘
```

Data source: `getMissionControlCalendarWeek()` already returns `always_running`, `next_up`, `jobs` with `next_run_at`, `interval_seconds`, `schedule_kind`. **Pure frontend visualization — no backend changes.**

**Schedule tab**: Paginated table (5-8 rows), relative "Next Run" times, colored status dots.
**Active Jobs tab**: Always Running + Next Up lists.

### 12.4 Focus

**Sub-tabs:** `[Queue (3)] [System Status]`

- Queue: paginated (5-8 items). Each item has **[Details]** expander showing approval context (parse `request_json` field already in response). Relative timestamps. Colored severity icons.
- System Status: stats grid + channel status dots.

### 12.5 Events

- Fixed-height container showing last N events that fit viewport.
- **Structured rendering** per event type (not raw JSON). `[▸ JSON]` expander per event.
- **Filter dropdown**: All / Board / Job / Approval / Channel / Mail.
- **"Load more" button** at bottom instead of scroll.
- Color-coded left border by event domain.

### 12.6 Mail

**2-column layout** (not 3). Thread list left, conversation right. Compose **inline at bottom**.

- Thread list: paginated (8-10 per page).
- Messages: paginated (most recent N). "Load earlier" at top.
- **"+ New Thread"** → modal (Subject + Participants multi-select dropdown). 2 fields.
- Reply: text area + attach + options gear + send. 3 controls at rest.
- Advanced options (sender/principal override) behind **[⚙ Options]** popover.
- **Sub-tabs on mail page:** `[Messages] [Leases]` — leases get their own view, not crammed into compose panel.
- Lease creation → modal with preset TTL buttons + holder/glob dropdowns.

### 12.7 Chatrooms

Same 2-column pattern as Mail. Moderation panel → **[⚙ Room Settings]** button → modal with sub-tabs `[Participants] [Leases] [Settings]`. Reactions → emoji picker, not hardcoded text-message buttons.

### 12.8 Cockpit

- Widget palette → **"+ Add Widget" dropdown** in toolbar.
- Page selector → **dropdown** in toolbar.
- **Edit mode toggle** `[✎ Edit]`. View mode = content only. Edit mode = drag handles, resize, remove.
- Widget bodies fixed-height. Too many widgets → use multiple dashboard pages.
- Import/Export/Restore → `[⋮ More]` dropdown in edit-mode toolbar.

---

## 13. NEW: Team Page (Agent Roster)

This is a **new tab** added to the nav rail. It replaces API-only agent management with a visual, caveman-friendly interface.

**Inspiration:** OpenClaw MC "Meet the Team" page — avatar cards with roles, descriptions, skill tags, visual hierarchy.

**Data source:** `listAgents()` — already wired in `api.ts`. Returns `agent_id`, `name`, `workspace_root`, `model_provider`, `model_id`, `tool_profile`, `created_at`, `updated_at`. **Pure frontend. No backend changes.**

### Layout

```
┌──────────────────────────────────────────────────────┐
│ Meet Your Agents                        [+ New Agent]│
├──────────────────────────────────────────────────────┤
│                                                       │
│  ┌─────────────────────────────────────────────────┐ │
│  │ 🤖  agent-alpha                                  │ │
│  │     Model: anthropic / claude-3.5-sonnet         │ │
│  │     Tools: standard                              │ │
│  │     Workspace: ~/projects/main                   │ │
│  │     [provider] [anthropic] [standard]            │ │  ← tag chips
│  │                            [Edit] [Role Card →]  │ │
│  └─────────────────────────────────────────────────┘ │
│                                                       │
│  ┌─────────────────────────────────────────────────┐ │
│  │ 🤖  agent-beta                                   │ │
│  │     Model: openai / gpt-4o                       │ │
│  │     Tools: restricted                            │ │
│  │     Workspace: ~/projects/secondary              │ │
│  │     [provider] [openai] [restricted]             │ │
│  │                            [Edit] [Role Card →]  │ │
│  └─────────────────────────────────────────────────┘ │
│                                                       │
│  pg 1/1                                              │
└──────────────────────────────────────────────────────┘
```

### Interactions

- **Agent cards** are paginated (4-6 per page depending on card height). No scroll.
- **[+ New Agent]** → creation modal:
  ```
  ┌── Create Agent ─────────────────────────┐
  │ Agent ID:      [___________________]     │
  │ Name:          [___________________]     │
  │ Provider:      [▼ anthropic        ]     │  ← dropdown
  │ Model:         [▼ claude-3.5-sonnet]     │  ← dropdown, filtered by provider
  │ Tool Profile:  [▼ standard         ]     │  ← dropdown
  │ Workspace:     [▼ ~/projects/main  ]     │  ← combo dropdown with history
  │                [Cancel]  [Create Agent]   │
  └──────────────────────────────────────────┘
  ```
  All dropdowns. Caveman proof.

- **[Edit]** → same modal, pre-filled with current values. Uses `update_agent` API (already exists in gateway, needs wrapper added to `api.ts`).

- **[Role Card →]** → expands or opens a detail view showing:
  - Agent's assigned jobs (from `listJobs` filtered by `agent_id`)
  - Agent's provider profile order (from `getAgentProviderProfileOrder`)
  - Recent activity summary
  - Auth profiles associated with this agent's providers

- **Per-page stats**: `Total Agents: 3  Providers: 2  Active Jobs: 5`

### Nav Rail Addition

```
Boards:     grid/kanban icon
Calendar:   calendar icon
Focus:      eye/target icon
Events:     pulse/waveform icon
Mail:       envelope icon
Chatrooms:  message-bubble icon
Team:       users/people icon        ← NEW
Cockpit:    dashboard/gauge icon
```

Team goes between Chatrooms and Cockpit — it's an infrastructure/config view, not a daily-ops view.

### API Wiring Needed (frontend only)

| Action | Endpoint | Wrapper Status |
|--------|----------|---------------|
| List agents | `GET /api/v1/agents` | `listAgents` exists in api.ts |
| Create agent | `POST /api/v1/agents` | Needs new wrapper in api.ts |
| Update agent | `POST /api/v1/agents/{agent_id}` | Needs new wrapper in api.ts |
| List jobs (for agent detail) | `GET /api/v1/jobs` | `listJobs` exists, filter client-side by agent_id |
| Provider profile order | `GET /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order` | `getAgentProviderProfileOrder` exists |
| Auth profiles | `GET /api/v1/auth/profiles` | `listAuthProfiles` exists |

**Backend changes needed: ZERO.** All endpoints exist. Just need 2 new wrappers in `api.ts` + corresponding types in `types.ts`.

---

## 14. FUTURE (v2): Memory / Journal Page

**NOT in scope for the design pass implementation.** Documented here for future planning.

The OpenClaw MC has a rich Memory page — journal-style document viewer with timestamped entries, decisions, issues, and action items on a timeline. CarsinOS has memory notes (`listMemoryNotes`, `createMemoryNote`) and an embeddings system, but no rich journal visualization.

**When NumquamOblita integration lands**, add a Memory tab to the nav rail:

```
Boards | Calendar | Focus | Events | Mail | Chatrooms | Team | Memory | Cockpit
```

**Conceptual design:**
- Left sidebar: timeline navigation by date
- Main content: rich document view of memory entries with timestamps, tags, source references
- Search: global memory search with semantic matching (requires new backend endpoint leveraging embeddings)
- Integration: link from mail thread "Summarize to Memory" action to the Memory page

**This requires backend work:** new endpoints for memory retrieval with filtering, timeline aggregation, and potentially a semantic search endpoint. File backend tickets when ready.

---

## 15. Implementation Guardrails

### Frontend-Only Rule (from API_CONTRACT.md)

- UI/UX tasks touch `src/features/*`, `src/ui/*`, and `src/styles.css` only.
- `src/lib/api.ts`, `src/lib/ws.ts`, and `src/types.ts` are read-only unless wiring a new screen (Team page needs 2 new wrappers).
- **Never change `crates/*`** in a frontend task.
- All HTTP calls go through `api.ts` wrappers. No raw `fetch()` in components.
- All types come from `types.ts`. No inline type definitions.

### What's Pure Frontend (no backend)

| Category | Examples |
|----------|---------|
| CSS/theming | Dark mode, all 5 themes, typography, motion, layout |
| Component restructure | Nav rail, topbar, sub-tabs, modals, pagination |
| New pages | Team page (API exists) |
| Dropdown conversion | All 11 identified text→dropdown conversions |
| Event rendering | Structured event cards (parse existing JSON) |
| Approval context | Parse existing `request_json` field |
| Calendar week grid | Visualize existing `getMissionControlCalendarWeek` data |
| Board filtering | Client-side filter on already-loaded cards |
| Relative timestamps | Frontend utility function |
| Toast system | New React component, replaces notice prop chain |
| Icons | Lucide npm dependency |
| Keyboard shortcuts | Frontend event listeners |

### What Needs Backend Tickets (NOT part of this pass)

| Feature | Missing | Priority |
|---------|---------|----------|
| List known principals endpoint | No `GET /api/v1/principals` | Low — can fake client-side |
| Message cursor pagination | Only `limit`, no `offset` | Medium — workaround exists |
| Memory/Journal integration | New NumquamOblita endpoints | v2 |

---

## 16. Updated Implementation Phasing

```
Phase 0: THEME TOKENS + DARK MODE
  CSS only. Highest impact. Unblocks everything.
  Checkpoint: post-green (typecheck, lint, build)

Phase 1: ZERO-SCROLL + LAYOUT RESTRUCTURE
  Shell collapse, nav rail, sub-tabs, pagination, modals.
  Checkpoint: post-green per sub-phase

Phase 2: DROPDOWN-FIRST + CAVEMAN SIMPLIFICATION
  Text→dropdown conversions, modal creation flows, confirmations.
  Checkpoint: post-green

Phase 3: UI PRIMITIVES + DESIGN SYSTEM
  Button, Input, Select, Toast, Modal, Icon, Skeleton, Badge, Pagination.
  Checkpoint: post-green

Phase 4: TEAM PAGE (NEW)
  New feature page. 2 new api.ts wrappers. Agent roster + creation modal.
  Checkpoint: post-green

Phase 5: PER-FEATURE VISUAL POLISH
  Cockpit → Boards → Focus → Events → Mail → Calendar (priority order).
  Checkpoint: post-green per feature

Phase 6: CALENDAR WEEK GRID
  Visual week timeline using existing getMissionControlCalendarWeek data.
  Checkpoint: post-green

Phase 7: MOTION + MICRO-INTERACTIONS
  Tab transitions, card drag, event fade-in, toast animations.

Phase 8: POWER-USER FEATURES
  Command palette, keyboard shortcuts, compact density, remaining themes.
```

---

*This document is now the complete design system + interaction architecture + implementation specification for Mission Control. The Laws (Section 11) are non-negotiable. Everything else serves them.*
