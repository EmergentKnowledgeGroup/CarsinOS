# AppDex Executable Ticket Backlog: Mission Control Option C (Server-Driven Catalogs)

Date: 2026-03-02  
Owner: AppDex  
Scope: Mission Control frontend hardcoded-config cleanup + gateway endpoint(s) needed to make model/provider choices server-driven (Option C).

References:
- Spec: `docs/MC_HARDCODED_CONFIG_OPTION_C_IMPLEMENTATION_SPEC.md`
- Triage: `docs/MC_HARDCODED_CONFIG_ACTION_PLAN.md`
- Raw audit: `docs/MC_FRONTEND_HARDCODED_CONFIG_CANDIDATES.md`

## Phase Order (Strict)
1. P0: Remove frontend hardcoded literals + add server-driven model catalogs.
2. P1: UX hardening + guardrails (no operator footguns).
3. P2: Optional follow-ups (non-blocking).

## Guardrails (Unskippable)
1. No secrets/tokens ever returned by catalog endpoints.
2. Missing provider auth must be a clear *provider auth* error (not “gateway token invalid”).
3. UI must still function when catalog fetch fails (manual entry fallback behind “Advanced”).
4. Do not add a “list tool profiles” endpoint until tool profiles are enforced by runtime policy.

## P0 Tickets (Ship Option C)

### MC-OC-FE-001 Constants + Storage Keys (No Live Users = OK to Reset)
Priority: P0  
Goal: Eliminate duplicated literals and scattered localStorage keys so behavior is deterministic and refactors are low-risk.  
Deliver:
1. Add `apps/mission-control/src/constants.ts` and move duplicated literals into named exports:
   - `DEFAULT_GATEWAY_URL`, `API_REQUEST_TIMEOUT_MS`
   - `WS_RECONNECT_INITIAL_MS`, `WS_RECONNECT_MAX_MS`, `WS_MAX_RECONNECT_ATTEMPTS`
   - `EVENT_STREAM_BUFFER_CAP`
2. Add `apps/mission-control/src/storageKeys.ts` with a single `STORAGE_KEYS` object and update all callsites to import it.
3. Rename keys to a consistent `mc-*` scheme (accepted because there are no live users).
Acceptance:
1. No duplicated “magic numbers/strings” remain across runtime/app shell/theme/ws/cockpit/onboarding.
2. No file uses bare-string localStorage keys; all use `STORAGE_KEYS.*`.
3. Local settings reset is expected and documented (no migration required).

### MC-OC-BE-001 Gateway: List Provider Models Endpoint (Option C Core)
Priority: P0  
Goal: Make the gateway the single source of truth for “which model IDs are available right now” per provider/profile, enabling accurate dropdowns and mid-session model switching.  
Deliver:
1. Add protocol contract types in `crates/carsinos-protocol`:
   - `ListProviderModelsQuery`
   - `ProviderModelResponse`
   - `ListProviderModelsResponse`
2. Add gateway route: `GET /api/v1/providers/models` with query:
   - `provider` (required)
   - `agent_id` (optional)
   - `auth_profile_id` (optional)
   - `refresh` (optional, default `false`)
3. Implement provider-specific upstream fetch:
   - `openai`/`vllm`: OpenAI-compatible `GET /v1/models`
   - `anthropic`: `GET /v1/models` with required headers
   - `ollama`: `GET /api/tags` mapped to model `name`
   - `mock`: stable static list
4. Add cache + bypass:
   - in-memory TTL cache keyed by `{provider}:{resolved_base_url}:{auth_profile_id_or_none}`
   - `refresh=true` bypasses cache and refreshes it
5. Enforce authz/audit/rate-limit parity with existing provider capability surfaces.
6. Add integration tests that cover:
   - `provider=mock` returns expected list
   - unknown provider returns `400`
   - missing/invalid provider auth returns stable `5xx` with a clear error code
   - `refresh=true` path exercises bypass (deterministic via mock/stub)
Acceptance:
1. Endpoint returns a stable `items[]` list of `{ model_id, label }` with no secret material.
2. Error mapping is operator-readable (`AUTH_REQUIRED`, `TIMEOUT`, `DEPENDENCY_UNAVAILABLE`, etc.).
3. Repeated UI calls do not hammer upstream providers (cache works).

### MC-OC-FE-002 Mission Control: Server-Driven Provider/Model Dropdowns
Priority: P0  
Goal: Remove hardcoded provider/model catalogs from the UI and fetch the real, account-specific catalogs from the gateway at runtime.  
Deliver:
1. Add Mission Control API wrappers in `apps/mission-control/src/lib/api.ts`:
   - `listProviderCapabilities(...)` (if not already present)
   - `listProviderModels(settings, { provider, agent_id?, auth_profile_id?, refresh? })`
2. Add corresponding TS response types in `apps/mission-control/src/types.ts`.
3. Update `apps/mission-control/src/features/team/TeamPage.tsx`:
   - provider dropdown populated from capabilities (hide `unconfigured`; optionally hide advanced providers behind “Advanced”)
   - model dropdown populated by `listProviderModels()` on provider change
   - show loading/error states; preserve a manual-entry escape hatch behind “Advanced”
4. Update onboarding:
   - `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx` uses capabilities for provider options
   - optional: model dropdown for local providers backed by `listProviderModels()` with free-text fallback
   - `apps/mission-control/src/features/onboarding/onboardingState.ts` removes duplicated provider hint sets (derive from capabilities or centralize in one module)
Acceptance:
1. UI shows the *actual* available model IDs for OpenAI/Anthropic/local providers, per the operator’s configured auth profiles.
2. Operator can change model selection mid-session without typing model IDs (where provider supports it).
3. If the gateway is unreachable, the operator can still proceed using manual entry (behind “Advanced”).

### MC-OC-UX-003 Tool Profile UI “Reality Check” (No Fake Contracts)
Priority: P0  
Goal: Stop implying tool profiles are enforced when they are currently just a label stored on agents.  
Deliver:
1. Update the tool-profile UI label/help text to clearly state “label only / not enforced yet”.
2. Keep the existing selector (or switch to free-text) but do not add a new “list tool profiles” endpoint yet.
Acceptance:
1. Operators are not misled into thinking tool profiles affect runtime permissions today.
2. No new backend contracts are added for tool profiles without enforcement.

## P1 Tickets (Hardening)

### MC-OC-BE-010 Gateway: Parameter Validation + 4xx Determinism
Priority: P1  
Goal: Avoid brittle `//` URLs, empty IDs, and unclear error responses when required parameters are missing or invalid.  
Deliver:
1. Strictly validate required query params for `/api/v1/providers/models` (and any related capability endpoints touched).
2. Standardize `400` error payloads for invalid provider/params.
Acceptance:
1. Invalid/missing params never trigger an upstream call.
2. Error responses are stable and test-covered.

### MC-OC-FE-011 Mission Control: Catalog Fetch UX Guardrails
Priority: P1  
Goal: Prevent operators from getting stuck or spam-clicking provider APIs.  
Deliver:
1. Debounce/cancel in-flight model fetch on rapid provider changes.
2. Add “Refresh models” button behind “Advanced” that calls `refresh=true`.
3. Cache catalog results client-side for a short TTL to reduce UI thrash (optional if gateway cache is sufficient).
Acceptance:
1. Rapid provider toggling does not produce race-condition UI state.
2. Refresh is explicit and does not occur on every keystroke.

## P2 Tickets (Related Follow-ups)

### MC-COCKPIT-001 Cockpit: State/Template/Runner Hardening (Spot-Check Follow-ups)
Priority: P2  
Goal: Remove a few cockpit fragilities found in static review to avoid future regressions.  
Deliver:
1. `useCockpitController.ts`: remove nested state updates in `deleteCockpitPage` and keep state transitions deterministic.
2. `cockpitLayout.ts`: fix `opsDefaultTemplate()` row packing to advance by row max height (not per-widget height).
3. `cockpitApiRunner.ts`: validate required path params before dispatch; never call API wrappers with empty IDs.
4. `cockpitDataSources.ts`: align `sampleFields`/shape hints with actual API response wrappers (for builder UX).
Acceptance:
1. Cockpit page delete does not mis-sequence `activeCockpitPageId` under StrictMode.
2. Ops template never overlaps widgets in mixed-height rows.
3. Custom widget runner fails fast with a clear error when params are missing.

## Validation (For Any PR That Implements This Backlog)

Backend:
- `cargo test --workspace --locked`

Mission Control:
- `cd apps/mission-control && npm run typecheck`
- `cd apps/mission-control && npm run lint`
- `cd apps/mission-control && npm run build`
