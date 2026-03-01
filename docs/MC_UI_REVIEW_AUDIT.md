# Mission Control UI Review Audit (No Fixes)

**Date (UTC):** 2026-03-01

## Scope
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx`
- `apps/mission-control/src/features/agentMail/MailPage.tsx`
- `apps/mission-control/src/features/boards/BoardsPage.tsx`
- `apps/mission-control/src/features/boards/BoardLane.tsx`
- `apps/mission-control/src/features/cockpit/CockpitPage.tsx`
- `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx`
- `apps/mission-control/src/features/events/EventsPage.tsx`
- `apps/mission-control/src/features/focus/FocusPage.tsx`
- `apps/mission-control/src/ui/Avatar.tsx` (new)
- `docs/DELTA_LOG.md` (new)

## Validations
- `apps/mission-control`: `npm run lint` (pass)
- `apps/mission-control`: `npm run typecheck` (pass)

## Real Bugs (Should Fix)
- `apps/mission-control/src/ui/usePagination.ts:2` + `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:80` + `apps/mission-control/src/features/agentMail/MailPage.tsx:95` + `apps/mission-control/src/features/boards/BoardsPage.tsx:63` + `apps/mission-control/src/features/boards/BoardLane.tsx:31` + `apps/mission-control/src/features/focus/FocusPage.tsx:73` Issue: systemic pagination bug. Current pagination is not clamped/reset, so when filtered/switched data has fewer pages, UI can land on an empty page while empty-state checks still read full-array length. Resolution: clamp page in `usePagination.getPage()` to `totalPages` and shift empty-state checks to `visibleItems.length === 0` where appropriate.
- `apps/mission-control/src/features/agentMail/MailPage.tsx:492` Issue: custom glob preset is unreachable because select handler ignores `""` values. Resolution: allow explicit `""` selection for Custom mode (or separate preset selection from actual pattern value).
- `apps/mission-control/src/features/agentMail/MailPage.tsx:464` Issue: reserve lease is fire-and-forget and closes modal immediately; failure loses form context. Resolution: await lease creation and close modal only on success.
- `apps/mission-control/src/features/boards/BoardsPage.tsx:138` Issue: owner kind includes `human` but create modal has no human ID field; metadata can be incomplete. Resolution: add human ID input when owner kind is `human` (or block submit without it).
- `apps/mission-control/src/features/boards/BoardsPage.tsx:336` Issue: agent owner dropdown in card editor renders `agent.name` without fallback; blank names produce blank options. Resolution: fallback to `agent.agent_id` when `agent.name` is empty.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:266` Issue: attachment input is not cleared after file selection; selecting the same file again may not trigger `onChange`. Resolution: clear with `event.currentTarget.value = ""` after reading selected files.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:37` Issue: `onRefresh` prop remains in interface but is unused in UI (dead wiring). Resolution: remove prop or reintroduce a refresh control.

## Intended Design (Not a Bug)
- `apps/mission-control/src/features/agentMail/MailPage.tsx:169` Item: principal override as dropdown-only follows the current law/preference for known-set selectors. Optional enhancement: add `Custom...` fallback for power users.
- `apps/mission-control/src/features/events/EventsPage.tsx:24` Item: `heartbeat` in domain model without a heartbeat filter chip is intentional because heartbeat visibility is separately controlled by `Show heartbeats`.
- `apps/mission-control/src/ui/Avatar.tsx:2` Item: fixed avatar palette + inline styles are intentional for identity consistency; theme-aware remap is optional. Minor follow-up: add explicit decorative accessibility semantics.
- `apps/mission-control/src/features/boards/BoardsPage.tsx:245` Item: `onSelectCard("")` sentinel is controller-dependent and currently tolerated by existing flow.

## Low-Priority Polish
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:171` Item: missing busy guards on ack/download/reaction can create duplicate idempotent requests. Resolution: add per-action in-flight disable states.
- `apps/mission-control/src/features/focus/FocusPage.tsx:193` Item: busy-guard parity is missing for retry/reconnect compared to approve/deny path. Resolution: route through same busy-state guard helper.
- `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:170` Item: widget control actions have no in-flight guard. Resolution: track busy state per action and disable until completion.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:101` + `apps/mission-control/src/features/agentMail/MailPage.tsx:119` Item: async handler wrapping is functionally acceptable due to controller-side handling but can still produce noisy unhandled rejection warnings in dev tooling. Resolution: add local `try/catch` wrapping for cleaner control flow.

## Summary
| Category | Count |
| --- | --- |
| Real bugs to fix | 7 (pagination clamping is 1 systemic fix) |
| Intended design | 4 |
| Low priority polish | 4 |

## Reviewer Pushback / Priority Nuance
- `apps/mission-control/src/features/boards/BoardsPage.tsx:245` (`onSelectCard("")`): currently functional but brittle against stricter controller/type contracts; consider promoting from low urgency to structural cleanup priority.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:37` (`onRefresh` dead prop): low user impact, but should still be treated as cleanup debt to avoid interface drift and misleading wiring in tests/docs.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:171` + `apps/mission-control/src/features/focus/FocusPage.tsx:193` + `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:170` (missing busy guards): currently idempotent and mostly low priority, but can become operational noise under rapid operator clicks or stricter rate limits.
- `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:101` + `apps/mission-control/src/features/agentMail/MailPage.tsx:119` (async wrapper polish): UX is acceptable today due to controller-side handling, but local `try/catch` is still recommended for cleaner dev-console and test behavior.
