# Mission Control — Hardcoded Config: Action Plan

Captured: 2026-03-01
Source audit: `docs/MC_FRONTEND_HARDCODED_CONFIG_CANDIDATES.md`

This document distills the raw audit into actionable work vs. items we're explicitly declining to change, with reasoning. Intended audience: an LLM agent (Codex) writing the implementation spec.

---

## DO: Create `src/constants.ts` — Single Source of Truth

The highest-value, lowest-risk change. Several values are duplicated across 4+ files with no shared constant. Centralizing them eliminates drift and makes future tuning trivial.

### What goes in:

```ts
// Network
export const DEFAULT_GATEWAY_URL = "http://127.0.0.1:18789";
export const API_REQUEST_TIMEOUT_MS = 15_000;

// WebSocket reconnect
export const WS_RECONNECT_INITIAL_MS = 750;
export const WS_RECONNECT_MAX_MS = 5_000;
export const WS_MAX_RECONNECT_ATTEMPTS = 40;

// Buffers
export const EVENT_STREAM_BUFFER_CAP = 400;
```

### Files to update (replace inline literals with imports):

| Constant | Files currently hardcoding it |
|----------|-------------------------------|
| `DEFAULT_GATEWAY_URL` | `lib/api.ts:68`, `app/AppShell.tsx:299`, `features/onboarding/useOnboardingController.ts:107`, `features/onboarding/steps/StepConnect.tsx:48` |
| `API_REQUEST_TIMEOUT_MS` | `lib/api.ts:84` |
| `WS_RECONNECT_*` | `lib/ws.ts:27`, `lib/ws.ts:138` |
| `WS_MAX_RECONNECT_ATTEMPTS` | `app/useGatewayEvents.ts:30`, `App.tsx:160` |
| `EVENT_STREAM_BUFFER_CAP` | `App.tsx:130` |

---

## DO: Create `src/storageKeys.ts` — Centralize localStorage Keys

There are 7+ bare string literals used as localStorage keys scattered across unrelated files. A collision or typo would cause silent bugs. Centralizing them is trivial and prevents that class of error entirely.

```ts
export const STORAGE_KEYS = {
  gatewayUrl: "mc-gateway-url",
  gatewayToken: "mc-gateway-token",
  gatewayUrlHistory: "mc-gateway-url-history",
  density: "mc-density",
  themeFamily: "mc-theme-family",
  themeMode: "mc-theme-mode",
  cockpitLayout: "mc-cockpit-layout",
  onboardingDismissedAt: "mc-onboarding-dismissed-at",
} as const;
```

### Files to update:

| Key | Current location |
|-----|-----------------|
| `mc-gateway-url` | `lib/runtime.ts:4` |
| `mc-gateway-token` | `lib/runtime.ts:5` |
| `mc-gateway-url-history` | `app/AppShell.tsx:74` |
| `mc-density` | `app/AppShell.tsx:98` |
| `mc-theme-family`, `mc-theme-mode` | `app/useTheme.ts:36`, `app/useTheme.ts:44` |
| `mc-cockpit-layout` | `features/cockpit/cockpitLayout.ts:1` |
| `mc-onboarding-dismissed-at` | `features/onboarding/onboardingState.ts:3` |

---

## DO: Make Provider/Model Catalogs Server-Driven

The onboarding wizard and team page hardcode provider names, model IDs, and tool profile lists. These already exist as gateway API responses. The frontend should fetch them instead of maintaining a parallel copy that drifts.

### Affected files:

- `features/onboarding/steps/StepProvider.tsx:131` — hardcoded provider choices
- `features/onboarding/onboardingState.ts:26` — duplicated hint set
- `features/team/TeamPage.tsx:37,46,52` — hardcoded provider/tool-profile/model lists

### Approach:

- Add API calls to fetch available providers, models, and tool profiles from the gateway (endpoints likely already exist: `/api/v1/providers`, `/api/v1/tool-profiles`, etc.)
- Replace hardcoded arrays with fetched data, falling back to a minimal default set if the gateway is unreachable during onboarding
- The onboarding wizard's `StepProvider` should query on mount and populate its dropdown from the response

---

## DECLINE: Pagination Page Sizes

The audit flags ~15 page sizes (`events: 12`, `focus: 6`, `calendar: 6`, `board lane: 8`, etc.). These are **layout-coupled design decisions**, not configuration. Each value is tuned to the component's visual density, scroll area, and card dimensions. Changing a page size without adjusting the surrounding layout produces broken UX (orphan rows, excessive whitespace, scroll jumpiness). If these ever need to change, the change happens alongside a design pass on that component — not via a config knob.

**No action.**

---

## DECLINE: Debounce Timings

Three debounce values (`300ms`, `250ms`, `280ms`) are flagged. The slight variation is intentional — each controller has different refresh cost characteristics. Exposing these as config invites footgun scenarios (setting `0` effectively DDoS's the gateway). These are performance-tuned internals.

**No action.**

---

## DECLINE: UI Truncation Caps

Cockpit widget caps, calendar "Next Up" cap, mail summary timeline cap, toast visible cap — these are all visual overflow guards. They exist because the layout has finite space. Making them configurable means operators can break the UI without understanding why.

**No action.**

---

## DECLINE: API Client Default Limits

The audit flags API defaults (`focus: 50`, `jobs: 100`, `approvals: 100`, `mail: 300`). These match server-side pagination defaults. If the server's limits change, the frontend values update to match — this is a code change, not a runtime config scenario. Adding a config layer here means operators could set client limits higher than server limits, causing silent truncation and confusion.

**No action.**

---

## DECLINE: Event Routing Rules & Heartbeat Filtering

The logic that determines "which WS events trigger a refresh" and "hide heartbeat events by default" is **application behavior**, not configuration. Making it configurable means operators can suppress critical events or flood the event stream with heartbeats they don't understand. This is core UX logic.

**No action.**

---

## DECLINE: Theme Fonts & Google Fonts URL

The theme system already provides user choice via the theme picker. The font imports and theme family definitions are design system internals. "Configurable fonts" means building a full theming engine — massive scope, near-zero user demand.

**No action** (though the theme families themselves need a design pass — some of them are rough).

---

## DECLINE: Build-time / Tauri Config

Tauri app name, window sizes, CSP allowlist, keyring service names — these are already in `tauri.conf.json`, which is the correct config surface. They're build-time constants by definition.

**No action.**

---

## DECLINE: Time Formatting Thresholds

The `formatRelative` breakpoints ("just now" < 60s, then m/h/d) are universal UX conventions. If localization becomes a requirement, the right tool is an i18n library, not a config file.

**No action.**

---

## DECLINE: Miscellaneous Small Literals

- **Upload MIME fallback `application/octet-stream`** — This is the MIME standard fallback. It's not config.
- **Gateway history max `8`** — Sane default, no user has ever wanted to configure how many URLs their URL dropdown remembers.
- **Onboarding dismiss window `24h`** — UX decision, not operator config.
- **Notification history cap `50`** — Just implemented, sane default, revisit if users complain.
- **Reaction emoji set, TTL presets, glob presets** — UX defaults. No operator is customizing the emoji picker.

**No action.**

---

## Summary

| Action | Scope | Risk | Value |
|--------|-------|------|-------|
| Create `src/constants.ts` | 8-10 files, import swaps only | Very low | Eliminates duplication of 6 scattered values |
| Create `src/storageKeys.ts` | 7-8 files, string replacements | Very low | Prevents localStorage key collisions |
| Server-driven catalogs | 3-4 files, add API fetch + fallback | Medium | Eliminates hardcoded provider/model drift |
| Everything else | — | — | Declined with reasoning above |
