# Mission Control v3: `App.tsx` Refactor + Modularization Spec (AppDex)

## Outcome (Dummy-CEO Summary)
- Today: `apps/mission-control/src/App.tsx` is ~3,745 lines and contains almost all UI + state + data fetching + WebSocket handling.
- That is not “slow” at runtime by itself, but it is slow and risky to change: hard to review, easy to break, and guaranteed merge-conflict bait.
- Goal: split it into clean modules (pages, features, hooks, and shared UI) without changing behavior.

## Constraints
- No feature redesign in this refactor.
- No new dependencies required.
- Keep all existing behavior (tabs, boards drag/drop, mail/chatrooms, cockpit layout studio, WS event tail).
- Keep validations green each step: `npm run typecheck`, `npm run lint`, `npm run build`, and `cargo check` in `apps/mission-control/src-tauri`.

## Current Snapshot (Facts)
- File: `apps/mission-control/src/App.tsx` (~126KB / 3,745 LOC).
- Tabs in-file: `boards`, `calendar`, `focus`, `events`, `mail`, `chatrooms`, `cockpit`.
- Only separate component today: `BoardLane()` (and it uses TanStack Virtual).
- `src/components/` exists but is empty.
- Lint warnings (not errors): `react-hooks/incompatible-library` around `useVirtualizer()`.

## Target Folder Structure
Create these directories and move code into them.

```text
apps/mission-control/src/
  app/
    App.tsx
    AppShell.tsx
    tabs.ts
    useAppController.ts
    useGatewayEvents.ts
  ui/
    Chip.tsx
    Surface.tsx
    InlineActions.tsx
    EmptyState.tsx
  utils/
    datetime.ts
    files.ts
    text.ts
  features/
    boards/
      BoardsPage.tsx
      BoardLane.tsx
      CardDrawer.tsx
      boardModel.ts
      useBoardsController.ts
    calendar/
      CalendarPage.tsx
      useCalendarController.ts
    focus/
      FocusPage.tsx
      useFocusController.ts
    events/
      EventsPage.tsx
      EventStream.tsx
    agentMail/
      MailPage.tsx
      ChatroomsPage.tsx
      ThreadPane.tsx
      MessagePane.tsx
      ComposeBox.tsx
      LeasesPane.tsx
      agentMailSummary.ts
      useAgentMailController.ts
    cockpit/
      CockpitPage.tsx
      cockpitLayout.ts
      widgets/
        HealthWidget.tsx
        FocusWidget.tsx
        BreakersWidget.tsx
        JobsWidget.tsx
        ChannelsWidget.tsx
        ProfilesWidget.tsx
        SkillsWidget.tsx
        PluginsWidget.tsx
        EventsWidget.tsx
```

Notes:
- Keep `src/lib/*` and `src/types.ts` as-is.
- If you want fewer files, collapse `calendar/focus/events` into a single `features/missionControl/` directory.

## Architecture Rules (Keep It Clean)
- Controllers (hooks) own side effects (API calls, timers, WS refresh triggers) and expose “state + actions”.
- `useAppController` remains the top-level app-shell controller (tabs + connection/session shell state) and composes feature controllers.
- `useMissionControlController` is the chosen single controller for Mission Control read models and actions in this refactor wave (calendar + focus + provider/plugin controls) to keep cross-surface orchestration centralized.
- Pages are mostly presentational: they render UI, call actions, and keep only tiny UI-local state (input drafts).
- Shared UI primitives go in `src/ui/` (buttons can stay as plain `<button>` for now; focus on structure).
- Pure helpers go in `src/utils/` or feature-local `*Model.ts` modules.

## Extraction Map (What Moves Where)
Move these helpers out of `App.tsx` first (pure functions, no React state):
- `normalizeWidgetSpan`, `sanitizeCockpitPages`, `loadCockpitPagesFromStorage`, `persistCockpitPagesToStorage` -> `features/cockpit/cockpitLayout.ts`.
- Board helpers `toCardsByColumn`, `withUpsertCard`, `withOptimisticMove` -> `features/boards/boardModel.ts`.
- `toInputDateTimeValue`, `fromInputDateTimeValue`, `formatDateTime` -> `utils/datetime.ts`.
- `fileToBase64`, `formatBytes` -> `utils/files.ts`.
- `truncateText`, `parsePrincipalCsv` -> `utils/text.ts`.
- `buildThreadSummaryNote` -> `features/agentMail/agentMailSummary.ts`.

Move these UI blocks into tab pages (minimize behavior change):
- `{activeTab === "boards" ? (...) : null}` -> `features/boards/BoardsPage.tsx`.
- `{activeTab === "calendar" ? (...) : null}` -> `features/calendar/CalendarPage.tsx`.
- `{activeTab === "focus" ? (...) : null}` -> `features/focus/FocusPage.tsx`.
- `{activeTab === "events" ? (...) : null}` -> `features/events/EventsPage.tsx`.
- `{activeTab === "mail" ? (...) : null}` -> `features/agentMail/MailPage.tsx`.
- `{activeTab === "chatrooms" ? (...) : null}` -> `features/agentMail/ChatroomsPage.tsx`.
- `{activeTab === "cockpit" ? (...) : null}` -> `features/cockpit/CockpitPage.tsx`.

Move these “action clusters” into controllers:
- Boards actions (`handleBoardChange`, `handleDropCard`, `handleCreateCard`, `saveCardDraft`, `runCard`, `uploadAsset`, `previewAsset`, plus `refreshBoard`/`queueBoardRefresh`) -> `features/boards/useBoardsController.ts`.
- Mission-control read models (`loadMissionControlReadModels`, `queueMissionControlRefresh`, `runCalendarJobNow`, `toggleCalendarJob`, `resolveFocusApproval`, `reconnectFocusChannel`, provider ordering, plugin/skill toggles) -> `app/useMissionControlController.ts` or split across `useCalendarController`, `useFocusController`.
- Agent mail actions (`loadAgentMailReadModels`, `queueAgentMailRefresh`, `loadMailThreadById`, `createMailThread`, `sendThreadMessage`, `acknowledgeMessage`, `acknowledgeRoomUnread`, `postRoomReaction`, `summarizeSelectedMailThread`, `downloadMailAttachment`, leases APIs) -> `features/agentMail/useAgentMailController.ts`.
- Cockpit actions (`addCockpitWidget`, `removeCockpitWidget`, `moveCockpitWidget`, `resizeCockpitWidget`, `resetCockpitLayout`, `addCockpitPage`, `exportCockpitLayout`, `importCockpitLayout`) -> `features/cockpit/useCockpitController.ts` (renderer remains split into widget components).

Move WebSocket wiring to a dedicated hook:
- The `connectGatewayEvents(...)` `useEffect` -> `app/useGatewayEvents.ts`.
- `useGatewayEvents` should accept stable callbacks.
- `useGatewayEvents` trigger: event stream append.
- `useGatewayEvents` trigger: board incremental updates.
- `useGatewayEvents` trigger: “refresh read models” debounced timers.

## PR Plan (Incremental, Low-Risk)
Each PR should be small enough to review and must keep the app running.

### PR 0: Baseline + Guardrails
- Optional: add a “no behavior change” checklist to the PR template if you have one.
- Add a short note in `apps/mission-control/README.md` (or create one) documenting `npm run typecheck`, `npm run lint`, `npm run build`, and `npm run tauri:dev`.

Acceptance:
- `npm run typecheck`
- `npm run lint`
- `npm run build`
- `cd src-tauri && cargo check`

### PR 1: Extract Pure Helpers
- Create `src/utils/*` and move pure helper functions out of `App.tsx`.
- Create `features/boards/boardModel.ts` and `features/cockpit/cockpitLayout.ts`.
- Keep exports local and update imports in `App.tsx`.

Acceptance:
- Same commands as PR 0.

### PR 2: Extract `BoardLane`
- Move `BoardLane` to `features/boards/BoardLane.tsx`.
- Add an explicit lint suppression for TanStack Virtual warnings if desired.
- Use `// eslint-disable-next-line react-hooks/incompatible-library` only on the `useVirtualizer` line(s).

Acceptance:
- Same commands as PR 0.

### PR 3: Extract Tab Pages (UI Only)
- Create page components that accept props (state + handlers) and render the existing JSX.
- Keep the data/state in the original `App.tsx` for this PR to minimize risk.
- After this PR, `App.tsx` should be mostly “layout + controller + tab switch”.

Acceptance:
- Same commands as PR 0.

### PR 4: Add Controllers (Move State Out of `App.tsx`)
- Introduce `app/useAppController.ts` that composes feature controllers.
- Move feature state and actions from `App.tsx` into `useBoardsController`.
- Move feature state and actions from `App.tsx` into `useAgentMailController`.
- Move feature state and actions from `App.tsx` into `useCockpitController` (optional).
- Keep `App.tsx` as a thin wrapper: `const { state, actions } = useAppController()`.
- Keep `App.tsx` as a thin wrapper: `return <AppShell ...>`.

Acceptance:
- Same commands as PR 0.

### PR 5: WebSocket Hook Cleanup
- Move WS logic into `app/useGatewayEvents.ts`.
- Avoid stale-closure bugs by using `useRef` to hold latest handlers, or by passing stable callbacks from controllers.
- Ensure cleanup closes the socket.

Acceptance:
- Same commands as PR 0.

### PR 6: UI Primitives + Naming
- Create `src/ui/*` for repeated patterns (chips, surfaces, empty states, action rows).
- Keep styles in `styles.css` for now.

Acceptance:
- Same commands as PR 0.

## Acceptance Criteria (End State)
- `App.tsx` <= ~200 lines and contains no large JSX blocks.
- Each tab is its own file, and each feature owns its own controller hook.
- No behavior changes: board drag/drop still works.
- No behavior changes: board card drawer editing, run, upload/preview assets still works.
- No behavior changes: calendar/focus/events render the same.
- No behavior changes: mail and chatrooms still load threads/messages, send messages, handle attachments, leases.
- No behavior changes: cockpit pages still persist to localStorage and widgets render correctly.
- Repo is “lint clean” or warnings are explicitly documented and localized.

## Optional Follow-Ups (Not Part of This Refactor)
- Switch board drag/drop from native HTML DnD to `@dnd-kit/*` (already installed) for better cross-platform behavior.
- Add component tests (React Testing Library): board move optimistic update.
- Add component tests (React Testing Library): mail thread selection + compose.
- Add component tests (React Testing Library): cockpit layout persistence.
