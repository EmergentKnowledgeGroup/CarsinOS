# Mission Control — Option C + Hardcoded Config Cleanup (Implementation Spec)

Date: 2026-03-01  
Related:
- Audit inventory: `docs/MC_FRONTEND_HARDCODED_CONFIG_CANDIDATES.md`
- Action plan / triage: `docs/MC_HARDCODED_CONFIG_ACTION_PLAN.md`

## Caveman Summary (Plain English)

- Today the UI has **lists typed into the code** (providers, models, “tool profiles”, localStorage keys, etc.).
- Those lists **go stale** and we have to keep fixing them by hand.
- Option C makes the gateway the **single source of truth** for “what models exist right now”, by asking OpenAI/Anthropic/local at runtime and handing that list to the UI.
- Since we are **not live** and have **no users**, we can safely **rename/reset localStorage keys** to clean things up without migrations.

## Scope (What We Will Do)

1) Mission Control frontend
- Add `apps/mission-control/src/constants.ts` and replace duplicated numeric/string literals with imports.
- Add `apps/mission-control/src/storageKeys.ts` and replace scattered localStorage key strings with imports (and **rename keys** for consistency since there are no users).
- Remove hardcoded provider/model catalogs in Mission Control screens and fetch them from the gateway.

2) Gateway + protocol (to support Option C)
- Add a new gateway endpoint to list models for a provider (`OpenAI`, `Anthropic`, and local connectors like `Ollama`/`vLLM`).
- Add matching request/response types to `crates/carsinos-protocol`.
- Add tests (gateway-level) that prove the endpoint works and errors correctly.

## Non-Goals (What We Will Not Do)

- We will not turn every “page size”, “debounce”, “truncate cap”, etc. into config. Those are UX/layout decisions (see decline rationale in `docs/MC_HARDCODED_CONFIG_ACTION_PLAN.md`).
- We will not pretend “tool profiles” are a real enforced server concept unless we also implement enforcement. Today `tool_profile` is only stored on agents and not used by runtime policy.

## Current Problem (Why Option C)

Mission Control currently hardcodes:
- Provider lists (ex: OpenAI / Anthropic / OpenRouter / Ollama / vLLM / Mock)
- Model ID lists per provider (ex: `gpt-4o`, `claude-*`, etc.)

Hardcoding is fragile because:
- Providers add/remove/rename models over time.
- Access is account-dependent (your OpenAI org may not have model X; your Anthropic org may have model Y).
- We want to switch models mid-session without typing IDs or relying on stale dropdown suggestions.

## Option C — Server-Driven Model Catalogs

### Desired UX outcome

- Anywhere the UI needs a “Model” dropdown, it should:
  1. Ask the gateway: “For provider P, what models are available right now?”
  2. Show those exact model IDs as choices.
  3. Allow changing selection at any time; the next run/request uses the new model ID.

This is already compatible with the backend run contract: `CreateRunRequest` supports `model_provider` + `model_id` per run (`crates/carsinos-protocol/src/lib.rs:780`).

### New Gateway Endpoint (Option C core)

Add:
- `GET /api/v1/providers/models`

Query params (v1):
- `provider` (required): `openai | anthropic | ollama | vllm | mock | openrouter | ...`
- `agent_id` (optional but recommended): used to resolve auth profile priority using the same logic as runs (`resolve_run_auth_profiles`).
- `auth_profile_id` (optional): forces a specific profile (same semantics as `CreateRunRequest.auth_profile_id`).
- `refresh` (optional bool, default `false`): bypass cache and re-fetch from upstream provider.

AuthZ + audit:
- Require bearer auth.
- Roles: `operator_admin` and `operator_readonly` (same as `/api/v1/providers/capabilities`).
- Audit event name: `provider.models.list`
- Audit resource:
  - If `provider` only: `provider:{provider}`
  - If `provider` + `auth_profile_id`: `provider:{provider}:auth_profile:{id}`

Rate limiting:
- Reuse the existing `run` endpoint-kind rate limiter (this endpoint fans out to provider APIs and should not be spammed).

### Response contract

Add to `crates/carsinos-protocol/src/lib.rs`:

- `ListProviderModelsQuery`
- `ProviderModelResponse`
- `ListProviderModelsResponse`

Proposed JSON shape:

```json
{
  "contract_version": "v1",
  "provider": "openai",
  "auth_profile_id": "ap_123",
  "items": [
    { "model_id": "gpt-4o", "label": "gpt-4o" },
    { "model_id": "o3-mini", "label": "o3-mini" }
  ]
}
```

Notes:
- `label` is allowed to equal `model_id` for now; it exists so we can add nicer display names later without breaking clients.
- We deliberately do **not** return any secret material.
- `auth_profile_id` is the resolved profile used for the upstream fetch, and may be `null` for providers that do not use auth profiles.

### Provider-specific upstream behavior (implementation detail)

The gateway should fetch upstream catalogs using the same base URL + credentials that runs would use:

- `openai`:
  - `GET {api_base_url}/v1/models` with `Authorization: Bearer <token>`
- `anthropic`:
  - `GET {api_base_url}/v1/models` with:
    - `x-api-key: <token>`
    - `anthropic-version: 2023-06-01` (match existing validation flow)
- `vllm` (OpenAI-compatible local):
  - `GET {base}/v1/models` (no auth unless we later support it explicitly)
- `ollama`:
  - `GET {base}/api/tags` and return model `name`s
- `mock`:
  - return a stable static list (ex: `["mock-echo-v1"]`)

Base URL sources:
- For providers that require auth (`openai`, `anthropic`, `openrouter`): use the resolved auth profile’s `api_base_url` when present, else provider default.
- For local providers (`ollama`, `vllm`): use provider defaults (currently `http://127.0.0.1:11434` and `http://127.0.0.1:8000` respectively, matching `crates/carsinos-providers` behavior).

### Caching strategy

Without caching, an eager UI can easily hammer providers. Add an in-memory cache in the gateway:
- Cache key: `{provider}:{resolved_base_url}:{auth_profile_id_or_none}`
- TTL: 5 minutes (tunable constant)
- `refresh=true` bypasses cache and updates it

### Error mapping

When upstream fetch fails, return a stable error surface:
- Invalid provider: `400 BAD_REQUEST`
- Upstream provider auth missing/invalid: `502 BAD_GATEWAY` with `error_code=AUTH_REQUIRED` (message must clearly say this is *provider* auth, not the gateway bearer token)
- Provider rate limit: `429` with `error_code=RATE_LIMITED` (if detectable)
- Timeout: `504` with `error_code=TIMEOUT`
- Network/provider down: `503` with `error_code=DEPENDENCY_UNAVAILABLE`

## Mission Control Frontend Changes

### 1) `apps/mission-control/src/constants.ts`

Create a small “single source of truth” constants module and replace duplicated literals with imports.

Include (initial set; expand only when duplication exists):
- `DEFAULT_GATEWAY_URL` (`http://127.0.0.1:18789`)
- `API_REQUEST_TIMEOUT_MS` (`15_000`)
- `WS_RECONNECT_INITIAL_MS` (`750`)
- `WS_RECONNECT_MAX_MS` (`5_000`)
- `WS_MAX_RECONNECT_ATTEMPTS` (`40`)
- `EVENT_STREAM_BUFFER_CAP` (`400`)

Acceptance:
- No more inline `750/5000/40/400/15_000` in multiple files.
- Placeholders (`http://127.0.0.1:18789`) import from the constant too.

### 2) `apps/mission-control/src/storageKeys.ts` (rename is OK)

Create `STORAGE_KEYS` and update all callsites.

Because we have **no live users**, we will rename keys to a consistent “mc-*” scheme and accept that settings reset.

Proposed keys (v1, stable):

```ts
export const STORAGE_KEYS = {
  gatewayUrl: "mc-gateway-url",
  gatewayUrlHistory: "mc-gateway-url-history",
  gatewayTokenFallback: "mc-gateway-token",
  density: "mc-density",
  themeName: "mc-theme-name",
  themeMode: "mc-theme-mode",
  cockpitPages: "mc-cockpit-pages",
  onboardingDismissedAtMs: "mc-onboarding-dismissed-at-ms",
} as const;
```

Files to update:
- `apps/mission-control/src/lib/runtime.ts` (connection + token fallback)
- `apps/mission-control/src/app/AppShell.tsx` (gateway history + density)
- `apps/mission-control/src/app/useTheme.ts` (theme keys)
- `apps/mission-control/src/features/cockpit/cockpitLayout.ts` (cockpit layout)
- `apps/mission-control/src/features/onboarding/onboardingState.ts` (dismissed-at key)

Notes:
- Tauri keychain storage (`apps/mission-control/src-tauri/src/lib.rs`) remains unchanged; only the browser fallback key is renamed.

### 3) Server-driven catalogs in the UI

#### Add API wrappers + types

Mission Control currently does not call:
- `GET /api/v1/providers/capabilities`
- `GET /api/v1/tools/capabilities`

Add to `apps/mission-control/src/lib/api.ts`:
- `listProviderCapabilities(settings, query?)`
- `listProviderModels(settings, { provider, agent_id?, auth_profile_id?, refresh? })` (new endpoint)

Add to `apps/mission-control/src/types.ts`:
- `ProviderCapabilityResponse` / `ListProviderCapabilitiesResponse`
- `ProviderModelResponse` / `ListProviderModelsResponse`

#### Replace hardcoded provider/model lists

Mission Control screens to update:

1) `apps/mission-control/src/features/team/TeamPage.tsx`
- Replace `PROVIDER_OPTIONS` with `listProviderCapabilities()` results (filter out `"unconfigured"`; optionally hide `"openrouter"` unless “Advanced” is open).
- Replace `MODELS_BY_PROVIDER` with a live fetch:
  - on provider change, call `listProviderModels()` and populate the model dropdown.
  - show loading + error state (and keep a manual entry escape hatch behind “Advanced” if the fetch fails).

2) `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx`
- Replace hardcoded local provider options (`ollama/vllm/mock`) with capabilities filtered to local providers.
- Optional: replace “Model ID” free-text with a dropdown backed by `listProviderModels()` for `ollama`/`vllm` (keep free-text as fallback).

3) `apps/mission-control/src/features/onboarding/onboardingState.ts`
- Replace duplicated local provider hint set with a helper derived from capabilities (or keep a small static set but colocate it in one module).

## Tool Profiles (Reality Check + Decision)

Today:
- `tool_profile` exists on agent records and is editable in Mission Control.
- It is not referenced by the gateway runtime for tool allow/deny decisions.

Therefore:
- We will **not** add a “list tool profiles” endpoint as if it were authoritative.
- Short-term UI tweak: keep the existing dropdown or switch to free-text, but label it clearly as “label only / not enforced yet”.

If we want real tool profiles later, that is a separate spec: define what a profile means (tool allowlists, risk modes, approval defaults), where it lives (runtime config vs DB), and enforce it in the run engine.

## Validation Plan (Post-Implementation)

Backend:
- `cargo test --workspace --locked`

Mission Control:
- `cd apps/mission-control && npm run typecheck`
- `cd apps/mission-control && npm run lint`
- `cd apps/mission-control && npm run build`

## Lift / Effort (Rough)

- `constants.ts` + replacements: low (hours)
- `storageKeys.ts` + key rename: low-to-medium (hours; touches several files)
- Provider capabilities wiring (frontend-only): medium (hours)
- Option C endpoint + protocol + tests: medium (1–2 focused days)
- End-to-end UI integration for dynamic model dropdowns: medium (0.5–1 day)

## Implementation Breakdown (Suggested PR chunks)

1) PR-1 (frontend cleanup)
- Add `apps/mission-control/src/constants.ts`
- Add `apps/mission-control/src/storageKeys.ts`
- Replace callsites (no behavior change except localStorage reset)

2) PR-2 (backend Option C)
- Add protocol contract types for model catalogs
- Add `GET /api/v1/providers/models` in gateway with caching + audit + rate limiting
- Add gateway integration tests + stubs

3) PR-3 (frontend Option C wiring)
- Add API wrappers + TS types
- Update `TeamPage` provider/model dropdowns to use the gateway
- Update onboarding local provider dropdown (and optionally local model dropdown)
