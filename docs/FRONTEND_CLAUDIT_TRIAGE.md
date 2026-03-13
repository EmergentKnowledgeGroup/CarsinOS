# Frontend Claudit Full Triage

Date: 2026-03-12
Source audit: `frontend_claudit.md`
Purpose: convert the raw audit into a trustworthy working triage for fix execution.

## Scope and trust boundary

- All `182` row IDs in audit sections `1-19` have now been re-triaged line by line.
- Sections `20` and `21` are narrative/summary material, not part of the `182` row-ID total. They are triaged separately below.
- This document supersedes the original audit summary counts and top-10 list.
- Verification method:
  - direct source reads
  - section-scoped subagent verification, capped at two concurrent section passes at a time
  - manual fold-in of every completed section result

## Final counts for the 182 row IDs

| Bucket | Count |
|---|---:|
| `CONFIRMED_BUG` | `24` |
| `VALID_FINDING_NOT_BUG` | `99` |
| `OVERSTATED_OR_NOT_A_BUG` | `43` |
| `RETRACTED_INVALID` | `12` |
| `NEEDS_LIVE_UI_VERIFICATION` | `4` |

## Working rules

- Hand Claude the `Confirmed Bug Queue` as the real defect list.
- Use `Valid Findings, Not Bugs` as optional UX/product/architecture cleanup, not as bug tickets.
- Do not hand `Overstated` or `Retracted` items to Claude as bugs.
- Gate `Needs Live UI Verification` items behind an actual rendered pass before anyone starts fixing them.

## Confirmed Bug Queue

These are the true fix-now items that survived re-triage.

- `SH-02` Silent redirect when Runbook is disabled while active.
- `SH-A2` Connection status dot lacks a reliable accessible name.
- `SH-A4` Incident toggle is unlabeled for assistive tech.
- `BD-02` Board card editor stays clickable during save/run.
- `BD-A1` Board drag-and-drop has no keyboard path.
- `BD-A4` Shared modal focus management bug affects board modals too.
- `CK-A1` Cockpit sidebar page tabs are not descriptively labeled.
- `CK-A2` Cockpit context menu has no keyboard affordance/focus semantics.
- `CK-A3` Cockpit widget reordering is pointer-only.
- `FO-02` Disabled Focus actions do not explain why they are disabled.
- `ML-02` Agent Mail attachments leak across thread switches.
- `ML-06` Agent Mail disclosure button lacks disclosure semantics.
- `CR-01` Chatroom reactions render raw shortcodes instead of emoji.
- `HP-02` Help page card buttons are indistinguishable to assistive tech.
- `ST-02` Strategy forms have no dirty-state protection.
- `ST-05` Strategy exposes filter-pending state in controller but never renders it.
- `ST-A1` Strategy filter chips do not expose pressed state.
- `ST-A4` Shared modal focus/ARIA bug affects Strategy modals too.
- `TM-03` Team manager selection has no cycle warning/prevention.
- `CN-02` Connector publish/unpublish lifecycle actions lack confirmation.
- `RB-04` Runbook linked-artifacts area has no empty state.
- `UI-08` Shared modal lacks dialog semantics, focus trap, and focus return.
- `UI-09` Toast stack lacks live-region semantics.
- `CP-02` Command Palette search input is unlabeled.

## Shared-Root Duplicates

These are worth fixing once at the root instead of treating as separate implementation tracks.

- Shared modal accessibility/focus bug:
  `UI-08`, `BD-A4`, `ST-A4`
- Missing/weak accessibility naming:
  `SH-A2`, `SH-A4`, `CK-A1`, `HP-02`, `CP-02`

## Needs Live UI Verification

These are plausible, but source alone is not enough.

- `CA-07` Calendar “Today” indicator may be visually weak.
- `CR-04` Chatroom header may lose context while scrolling.
- `AS-05` Assistant toolbar may feel cramped in real layout.
- `LF-07` Live Feed virtualization estimate may cause visible scroll issues.

## Section-by-Section Buckets

### 1. App Shell & Navigation

- `CONFIRMED_BUG`: `SH-02`, `SH-A2`, `SH-A4`
- `VALID_FINDING_NOT_BUG`: `SH-03`, `SH-04`, `SH-05`, `SH-09`, `SH-10`, `SH-11`, `SH-14`
- `OVERSTATED_OR_NOT_A_BUG`: `SH-01`, `SH-06`, `SH-07`, `SH-A1`, `SH-A3`, `SH-A5`
- `RETRACTED_INVALID`: `SH-08`, `SH-12`, `SH-13`

### 2. Boards

- `CONFIRMED_BUG`: `BD-02`, `BD-A1`, `BD-A4`
- `VALID_FINDING_NOT_BUG`: `BD-01`, `BD-03`, `BD-04`, `BD-05`, `BD-06`, `BD-08`, `BD-09`, `BD-A3`
- `OVERSTATED_OR_NOT_A_BUG`: `BD-10`
- `RETRACTED_INVALID`: `BD-07`, `BD-A2`

### 3. Cockpit

- `CONFIRMED_BUG`: `CK-A1`, `CK-A2`, `CK-A3`
- `VALID_FINDING_NOT_BUG`: `CK-03`, `CK-04`, `CK-06`, `CK-07`, `CK-08`, `CK-10`
- `OVERSTATED_OR_NOT_A_BUG`: `CK-02`, `CK-05`, `CK-A4`
- `RETRACTED_INVALID`: `CK-01`, `CK-09`

### 4. Focus

- `CONFIRMED_BUG`: `FO-02`
- `VALID_FINDING_NOT_BUG`: `FO-03`, `FO-05`
- `OVERSTATED_OR_NOT_A_BUG`: `FO-01`, `FO-06`
- `RETRACTED_INVALID`: `FO-04`

### 5. Calendar

- `VALID_FINDING_NOT_BUG`: `CA-01`, `CA-03`, `CA-04`, `CA-06`, `CA-A2`
- `OVERSTATED_OR_NOT_A_BUG`: `CA-02`, `CA-05`, `CA-08`, `CA-A1`, `CA-A3`
- `NEEDS_LIVE_UI_VERIFICATION`: `CA-07`

### 6. Events

- `VALID_FINDING_NOT_BUG`: `EV-01`, `EV-03`
- `OVERSTATED_OR_NOT_A_BUG`: `EV-04`
- `RETRACTED_INVALID`: `EV-02`

### 7. Agent Mail

- `CONFIRMED_BUG`: `ML-02`, `ML-06`
- `VALID_FINDING_NOT_BUG`: `ML-03`, `ML-05`, `ML-08`, `ML-09`, `ML-10`
- `OVERSTATED_OR_NOT_A_BUG`: `ML-01`, `ML-04`, `ML-07`, `ML-11`

### 8. Chatrooms

- `CONFIRMED_BUG`: `CR-01`
- `VALID_FINDING_NOT_BUG`: `CR-03`, `CR-05`, `CR-06`, `CR-07`, `CR-08`, `CR-09`, `CR-10`
- `OVERSTATED_OR_NOT_A_BUG`: `CR-02`
- `NEEDS_LIVE_UI_VERIFICATION`: `CR-04`

### 9. Assistant

- `VALID_FINDING_NOT_BUG`: `AS-01`, `AS-02`, `AS-03`, `AS-04`, `AS-06`, `AS-08`
- `OVERSTATED_OR_NOT_A_BUG`: `AS-07`, `AS-09`
- `NEEDS_LIVE_UI_VERIFICATION`: `AS-05`

### 10. Help

- `CONFIRMED_BUG`: `HP-02`
- `VALID_FINDING_NOT_BUG`: `HP-01`
- `OVERSTATED_OR_NOT_A_BUG`: `HP-04`, `HP-06`
- `RETRACTED_INVALID`: `HP-03`, `HP-05`

### 11. Strategy

- `CONFIRMED_BUG`: `ST-02`, `ST-05`, `ST-A1`, `ST-A4`
- `VALID_FINDING_NOT_BUG`: `ST-01`, `ST-04`, `ST-06`, `ST-07`, `ST-08`, `ST-09`, `ST-10`, `ST-11`
- `OVERSTATED_OR_NOT_A_BUG`: `ST-03`, `ST-A2`, `ST-A3`

### 12. Team

- `CONFIRMED_BUG`: `TM-03`
- `VALID_FINDING_NOT_BUG`: `TM-01`, `TM-02`, `TM-04`, `TM-05`, `TM-07`
- `OVERSTATED_OR_NOT_A_BUG`: `TM-06`

### 13. Connectors

- `CONFIRMED_BUG`: `CN-02`
- `VALID_FINDING_NOT_BUG`: `CN-01`, `CN-03`, `CN-04`, `CN-05`, `CN-06`, `CN-08`
- `OVERSTATED_OR_NOT_A_BUG`: `CN-07`, `CN-09`

### 14. Memory

- `VALID_FINDING_NOT_BUG`: `ME-01`, `ME-03`, `ME-04`, `ME-05`, `ME-06`, `ME-07`, `ME-09`, `ME-10`
- `OVERSTATED_OR_NOT_A_BUG`: `ME-02`, `ME-08`

### 15. Runbook

- `CONFIRMED_BUG`: `RB-04`
- `VALID_FINDING_NOT_BUG`: `RB-03`, `RB-05`, `RB-06`, `RB-07`, `RB-08`
- `OVERSTATED_OR_NOT_A_BUG`: `RB-01`, `RB-02`, `RB-09`

### 16. Live Feed Drawer

- `VALID_FINDING_NOT_BUG`: `LF-02`, `LF-04`, `LF-05`, `LF-06`, `LF-09`
- `OVERSTATED_OR_NOT_A_BUG`: `LF-01`, `LF-03`, `LF-08`
- `RETRACTED_INVALID`: `LF-10`
- `NEEDS_LIVE_UI_VERIFICATION`: `LF-07`

### 17. Command Palette

- `CONFIRMED_BUG`: `CP-02`
- `VALID_FINDING_NOT_BUG`: `CP-01`, `CP-05`
- `OVERSTATED_OR_NOT_A_BUG`: `CP-03`, `CP-04`

### 18. Shared UI Primitives

- `CONFIRMED_BUG`: `UI-08`, `UI-09`
- `VALID_FINDING_NOT_BUG`: `UI-01`, `UI-02`, `UI-03`, `UI-04`, `UI-05`, `UI-06`, `UI-07`, `UI-10`

### 19. API Layer & OpsUxConfig

- `VALID_FINDING_NOT_BUG`: `AP-01`, `AP-02`, `AP-03`

## Narrative Claims Outside the 182 Row IDs

### Section 20: Cross-Cutting Issues

- `Silent Async Failures`: `OVERSTATED_OR_NOT_A_BUG`
- `No Dirty Form Tracking`: `CONFIRMED_SYSTEMIC_ISSUE`
- `Missing Responsive Design`: `NEEDS_LIVE_UI_VERIFICATION`
- `Accessibility Gaps`: `OVERSTATED_OR_NOT_A_BUG` as a blanket claim
- `Props Drilling`: `VALID_FINDING_NOT_BUG`
- `Inconsistent Empty States`: `VALID_FINDING_NOT_BUG`

### Section 21: Summary & Severity Matrix

- `SUMMARY-01`: `OVERSTATED_OR_NOT_A_BUG`
- `SUMMARY-02`: `OVERSTATED_OR_NOT_A_BUG`
- `SUMMARY-03`: `OVERSTATED_OR_NOT_A_BUG`
- `SUMMARY-04`: `OVERSTATED_OR_NOT_A_BUG`
- `SUMMARY-05`: `OVERSTATED_OR_NOT_A_BUG`

## What Claude Should Work From

1. Start with the `Confirmed Bug Queue`.
2. Treat `Needs Live UI Verification` as hold items until he sees the rendered app.
3. Pull from `Valid Findings, Not Bugs` only if he wants to include polish or UX cleanup beyond defect repair.
4. Ignore the original audit’s summary totals and top-10 list. They are not trustworthy planning inputs.
