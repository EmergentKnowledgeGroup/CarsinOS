# UI Polish Tiers — Visual Fixes from Claudit Triage

Source: `FRONTEND_CLAUDIT_TRIAGE.md` "Valid Findings, Not Bugs" bucket
Scope: Pure visual/UI polish — CSS, labels, visual feedback, layout. No architecture, no backend, no feature gaps.

---

## Tier 1: Quick Visual Wins (text/label/color fixes)

| ID | Section | What | Fix |
|----|---------|------|-----|
| SH-10 | App Shell | Scheduler chip shows red ("down") when just paused | Change tone to "warning" or "neutral" |
| FO-05 | Focus | Severity icon AND chip are redundant | Remove one |
| AS-04 | Assistant | Chips show raw UUID session/run IDs | Truncate or format |
| AS-08 | Assistant | Cmd+Enter shortcut exists but UI doesn't show it | Add hint text near Send |
| ML-05 | Mail | "Ack" button label — nobody knows what it means | Tooltip or rename to "Acknowledge" |
| CR-03 | Chatrooms | "Mention recipients" label is wrong — it adds recipients | Fix label |
| CR-05 | Chatrooms | "Reserve Workspace" — zero context on what it does | Add tooltip or subtitle |
| ME-04 | Memory | Health mismatch warning is cryptic jargon | Rewrite to plain language |
| ME-06 | Memory | "Read-only in this phase" with no explanation | Add what phase means |
| ME-10 | Memory | "Truncated" chip with no explanation of limits | Add tooltip |
| RB-05 | Runbook | "Partial" chip — partial what? | Add tooltip |
| LF-04 | Live Feed | "Critical+High" filter name is confusing | Rename to "Critical & High" or similar |

## Tier 2: Visual Feedback Improvements (loading states, indicators)

| ID | Section | What | Fix |
|----|---------|------|-----|
| BD-01 | Boards | No loading indicator on any board operation | Add spinner/skeleton |
| BD-09 | Boards | No "X of Y cards" count | Add pagination text |
| CA-01 | Calendar | Calendar buttons lack "Working..." text (other pages have it) | Add consistent loading text |
| CK-07 | Cockpit | Stale data ">15min" warning is plain text | Add warning styling/icon |
| RB-06 | Runbook | Disabled action reason shown in `<small>` — barely visible | Style with warning color/icon |
| RB-07 | Runbook | Refresh button has no animation while refreshing | Add spin class |
| LF-05 | Live Feed | Undo window has no timer/countdown | Add progress indicator |
| LF-09 | Live Feed | Collapsed Live Feed shows only "Unread: 5" | Add severity breakdown |
| EV-03 | Events | Events page has no loading state at all | Add skeleton |
| ME-05 | Memory | Facts list truncated to 6 with no "show more" | Add expand button |
| ME-07 | Memory | Turn pills show only ID — no timestamp or context | Add metadata |

## Tier 3: Layout & Consistency Polish

| ID | Section | What | Fix |
|----|---------|------|-----|
| CK-08 | Cockpit | Widget display patterns vary wildly (grid vs list vs cards) | Consistency pass |
| CK-10 | Cockpit | List item heights: 44px, 38px, 32px with no clear reason | Standardize |
| CK-06 | Cockpit | "Enable Strategy Hub" text with no link to Settings | Add link |
| ST-08 | Strategy | Goal edit in nav, project edit in detail header — inconsistent | Move to consistent position |
| CN-05 | Connectors | "Shared default" vs agent-scoped auth bindings look identical | Add visual grouping |
| CN-08 | Connectors | Tables have no sort indicators | Add arrows |
| ST-09 | Strategy | Blocked reason field appears/disappears with no hint why | Add conditional hint |

## Extras: Confirmed Bug Queue — Frontend-Only Fixes

Source: Dex's triage `FRONTEND_CLAUDIT_TRIAGE.md` "Confirmed Bug Queue" — items fixable purely in frontend code.

### Group A: Shared Modal Accessibility (fix once, clears 3 IDs)

| ID | Section | What | Fix |
|----|---------|------|-----|
| UI-08 | Shared UI | Modal lacks `role="dialog"`, `aria-modal`, focus trap, focus return | Add dialog semantics + focus trap to Modal.tsx |
| BD-A4 | Boards | Board modals inherit broken Modal | Fixed by UI-08 |
| ST-A4 | Strategy | Strategy modals inherit broken Modal | Fixed by UI-08 |

### Group B: Toast Live Region

| ID | Section | What | Fix |
|----|---------|------|-----|
| UI-09 | Shared UI | ToastStack has no `role="alert"` or `aria-live` | Add live-region semantics to Toast.tsx |

### Group C: Accessible Naming (fix pattern, clears 6 IDs)

| ID | Section | What | Fix |
|----|---------|------|-----|
| SH-A2 | App Shell | Connection status dot has no `aria-label` | Add aria-label |
| SH-A4 | App Shell | Incident mode toggle unlabeled for AT | Add aria-label to checkbox |
| CK-A1 | Cockpit | Sidebar page tabs are single-letter, no descriptive label | Add aria-label |
| HP-02 | Help | "Open" buttons are generic — no tab name for AT | Include tab name in button text |
| CP-02 | Command Palette | Search input has no `<label>` or `aria-label` | Add aria-label |
| ML-06 | Mail | Options disclosure button lacks `aria-expanded` | Add disclosure semantics |
| ST-A1 | Strategy | Filter chips have no `aria-pressed` | Add aria-pressed |

### Group D: Individual Confirmed Bugs

| ID | Section | What | Fix |
|----|---------|------|-----|
| FO-02 | Focus | Disabled buttons don't explain why they're disabled | Add title/tooltip with reason |
| ST-05 | Strategy | `isFilterTransitionPending` exposed but never rendered | Render pending indicator |
| RB-04 | Runbook | Linked-artifacts area has no empty state | Add EmptyState fallback |
