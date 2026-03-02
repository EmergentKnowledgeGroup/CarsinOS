# Mission Control Frontend — Hardcoded Values That Could Be Config

Captured: 2026-03-01

Scope: `apps/mission-control/` (Vite + React + Tauri).

This doc lists frontend literals that are plausible candidates for config (build-time env, operator/user prefs, or server-provided catalogs). File references are repo-relative `path:line`.

## Network / Endpoints / Auth

- Default Gateway URL + port `18789`: `apps/mission-control/src/features/onboarding/useOnboardingController.ts:107`, `apps/mission-control/src/features/onboarding/steps/StepConnect.tsx:48`, `apps/mission-control/src/app/AppShell.tsx:299`, `apps/mission-control/src/lib/api.ts:68`.
- Gateway scheme fallback forces `http://` when missing: `apps/mission-control/src/lib/api.ts:61`.
- Gateway request timeout `15_000ms`: `apps/mission-control/src/lib/api.ts:84`.
- WS path hardcoded to `/api/v1/ws` (and `/api/v1/*` is baked into every route): `apps/mission-control/src/lib/api.ts:916`, example route `apps/mission-control/src/lib/api.ts:143`.
- Provider base URL placeholders (may want to be defaults sourced from runtime config): `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx:200`, `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx:230`.

## Reconnect / Streaming Behavior

- WS reconnect backoff constants (start `750ms`, exponential, max `5000ms`): `apps/mission-control/src/lib/ws.ts:27`, `apps/mission-control/src/lib/ws.ts:138`.
- Default max reconnect attempts `40`: `apps/mission-control/src/app/useGatewayEvents.ts:30`, explicit usage `apps/mission-control/src/App.tsx:160`.
- Event stream buffer cap `400`: `apps/mission-control/src/App.tsx:130`.
- Event routing rules (“which events trigger refresh”) are hardcoded by prefix: `apps/mission-control/src/App.tsx:133`, `apps/mission-control/src/App.tsx:135`.
- Heartbeat filtering rule (`heartbeat.*` hidden unless toggled): `apps/mission-control/src/App.tsx:116`.

## Limits, Debounces, Page Sizes, Caps

### API client defaults (limits)

- Focus default `50`: `apps/mission-control/src/lib/api.ts:228`.
- Memory notes default `20`: `apps/mission-control/src/lib/api.ts:238`.
- Jobs list default `100`: `apps/mission-control/src/lib/api.ts:262`.
- Approvals list default `100`: `apps/mission-control/src/lib/api.ts:308`.
- Agent mail messages default `300`: `apps/mission-control/src/lib/api.ts:625`.

### Controller/UI overrides (limits)

- Focus `100`, jobs `200`, approvals `200`: `apps/mission-control/src/app/useMissionControlController.ts:157`, `apps/mission-control/src/app/useMissionControlController.ts:158`, `apps/mission-control/src/app/useMissionControlController.ts:159`.
- Agent mail thread messages `500`: `apps/mission-control/src/features/agentMail/useAgentMailController.ts:91`.
- Agent mail threads list limit `300` (direct + room): `apps/mission-control/src/features/agentMail/useAgentMailController.ts:111`, `apps/mission-control/src/features/agentMail/useAgentMailController.ts:118`.

### Debounce / refresh delays

- Mission Control refresh debounce `300ms`: `apps/mission-control/src/app/useMissionControlController.ts:223`.
- Boards refresh debounce `250ms`: `apps/mission-control/src/features/boards/useBoardsController.ts:118`.
- Agent Mail refresh debounce `280ms`: `apps/mission-control/src/features/agentMail/useAgentMailController.ts:144`.

### Per-page pagination sizes

- Events page size `12`: `apps/mission-control/src/features/events/EventsPage.tsx:22`.
- Focus page size `6`: `apps/mission-control/src/features/focus/FocusPage.tsx:16`.
- Calendar schedule page size `6`: `apps/mission-control/src/features/calendar/CalendarPage.tsx:28`.
- Board lane page size `8`: `apps/mission-control/src/features/boards/BoardLane.tsx:16`.
- Board assets page size `6`: `apps/mission-control/src/features/boards/BoardsPage.tsx:13`.
- Team page size `5`: `apps/mission-control/src/features/team/TeamPage.tsx:10`.
- Mail page sizes (threads/messages/leases = `8/10/6`): `apps/mission-control/src/features/agentMail/MailPage.tsx:19`, `apps/mission-control/src/features/agentMail/MailPage.tsx:20`, `apps/mission-control/src/features/agentMail/MailPage.tsx:21`.
- Chatrooms page sizes (rooms/messages = `8/10`): `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:18`, `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:19`.

### UI truncation caps (slices)

- Cockpit widget caps: `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:140`, `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:160`, `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:173`, `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:192`, `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx:433`.
- Calendar “Next Up” cap `5`: `apps/mission-control/src/features/calendar/CalendarPage.tsx:197`.
- Agent mail summary timeline cap `12` + per-message truncate `280`: `apps/mission-control/src/features/agentMail/agentMailSummary.ts:20`, `apps/mission-control/src/features/agentMail/agentMailSummary.ts:25`.
- Toast max visible `4` + auto-dismiss durations (info `4000ms`, error `8000ms`): `apps/mission-control/src/ui/Toast.tsx:16`, `apps/mission-control/src/ui/Toast.tsx:52`.
- Notification history cap `50`: `apps/mission-control/src/ui/useToasts.ts:8`.

### Other “tunable” literals that impact behavior

- “Always running” threshold uses `interval_seconds <= 300`: `apps/mission-control/src/app/useMissionControlController.ts:188`.
- Upload MIME fallback `application/octet-stream`: `apps/mission-control/src/features/boards/useBoardsController.ts:235`.

## Static “Catalog” Data

- Provider/tool-profile/model ID lists are hardcoded (likely should be server-provided or config-driven): `apps/mission-control/src/features/team/TeamPage.tsx:37`, `apps/mission-control/src/features/team/TeamPage.tsx:46`, `apps/mission-control/src/features/team/TeamPage.tsx:52`.
- Onboarding local provider choices are hardcoded: `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx:131`, hint-set duplication `apps/mission-control/src/features/onboarding/onboardingState.ts:26`.
- Agent mail UI presets:
  - TTL presets: `apps/mission-control/src/features/agentMail/MailPage.tsx:23`.
  - Glob presets: `apps/mission-control/src/features/agentMail/MailPage.tsx:31`.
  - Reaction emoji set: `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx:21`.
- Cockpit default pages + widget palette are hardcoded: `apps/mission-control/src/features/cockpit/cockpitLayout.ts:46`, `apps/mission-control/src/features/cockpit/cockpitLayout.ts:190`.

## Defaults + Preference Persistence

- LocalStorage keys (could be centralized into one “settings schema” module):
  - Runtime connection + token fallback: `apps/mission-control/src/lib/runtime.ts:4`, `apps/mission-control/src/lib/runtime.ts:5`.
  - Onboarding dismissed-at key: `apps/mission-control/src/features/onboarding/onboardingState.ts:3`.
  - Gateway URL history: `apps/mission-control/src/app/AppShell.tsx:74`.
  - Density setting: `apps/mission-control/src/app/AppShell.tsx:98`.
  - Theme name/mode: `apps/mission-control/src/app/useTheme.ts:36`, `apps/mission-control/src/app/useTheme.ts:44`.
  - Cockpit layout: `apps/mission-control/src/features/cockpit/cockpitLayout.ts:1`.
- “Default values” that likely want to be configurable:
  - Onboarding defaults (agent id/name/workspace/tool profile/provider names): `apps/mission-control/src/features/onboarding/useOnboardingController.ts:114`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:115`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:116`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:117`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:127`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:128`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:130`, `apps/mission-control/src/features/onboarding/useOnboardingController.ts:134`.
  - Onboarding dismiss window `24h`: `apps/mission-control/src/features/onboarding/onboardingState.ts:4`.
  - Gateway history max `8`: `apps/mission-control/src/app/AppShell.tsx:75`.

## Theme / Fonts

- Theme families + font URLs (Google Fonts dependency): `apps/mission-control/src/app/useTheme.ts:14`, `apps/mission-control/src/app/useTheme.ts:23`.
- Global CSS imports Google Fonts directly: `apps/mission-control/src/styles.css:7`.

## Build-time / Desktop App Config

- Tauri app config (name/version/id/dev port/window sizes/CSP allowlist): `apps/mission-control/src-tauri/tauri.conf.json:3`, `apps/mission-control/src-tauri/tauri.conf.json:8`, `apps/mission-control/src-tauri/tauri.conf.json:16`, `apps/mission-control/src-tauri/tauri.conf.json:25`.
- Keyring naming for token storage: `apps/mission-control/src-tauri/src/lib.rs:3`, `apps/mission-control/src-tauri/src/lib.rs:4`.
- HTML title + icon: `apps/mission-control/index.html:5`, `apps/mission-control/index.html:7`.

## Optional: Time Formatting Thresholds

- Relative-time thresholds are hardcoded (UX/config candidate for consistency/localization): `apps/mission-control/src/utils/datetime.ts:31`.
