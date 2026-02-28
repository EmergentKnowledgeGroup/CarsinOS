# Mission Control Onboarding Setup Wizard Spec (OpenClaw-Adapted)

## Why This Exists (Dummy-CEO Summary)
Mission Control currently assumes a user already knows gateway URLs, bearer tokens, providers, profiles, and routing order.
That is acceptable for power users, but it is a hard stop for first-time users.

This wizard is the first-run "guided setup lane" for zero-experience users:
1. Connect to gateway (local quickstart or existing remote).
2. Ensure at least one agent exists.
3. Attach at least one provider path (Anthropic, OpenAI, or local connector mode).
4. Apply agent routing safely.
5. Confirm green status and drop user into work surfaces.

This spec intentionally adapts proven OpenClaw onboarding patterns:
- flow choice (`Quickstart` vs `Manual`)
- preflight checks before setup actions
- existing-config-aware defaults
- explicit review/confirm before applying destructive changes
- clear finish state with direct next action

## Success Criteria
- A first-time user reaches "usable setup complete" in under 2 minutes in Quickstart mode.
- Wizard supports both:
  - local quickstart
  - connect to existing gateway
- No secrets are persisted in localStorage or logs.
- Wizard can be reopened at any time and can safely resume partial progress.
- Build gates remain green:
  - `npm run typecheck`
  - `npm run lint`
  - `npm run build`
  - `cargo check` in `apps/mission-control/src-tauri`

## Scope (V1)
- UI-only onboarding overlay inside Mission Control.
- Minimal API wrapper additions needed for setup automation.
- Provider onboarding paths:
  - Anthropic setup token ingest
  - OpenAI OAuth (PKCE)
  - Local connector path (no remote OAuth required; routing/setup still explicit)

## Non-Goals (V1)
- Full environment/config administration panel.
- Full channel provisioning in the onboarding lane.
- Multi-user org/team lifecycle management.

## Supported Provider Paths (V1)
The wizard is **not Anthropic-only**.

### Path A (Recommended): Anthropic setup token
- Endpoint: `POST /api/v1/auth/anthropic/setup-token/ingest`
- Outcome: enabled Anthropic profile created and ready for routing.

### Path B: OpenAI OAuth
- Endpoints:
  - `POST /api/v1/auth/openai/oauth/start`
  - `POST /api/v1/auth/openai/oauth/finish`
- Outcome: enabled OpenAI OAuth profile created and ready for routing.

### Path C: Local connector
- Supports local-first usage with non-OAuth setup.
- For local providers that do not require auth profile to run, wizard marks provider step complete without secret ingestion.
- If a profile is still desired (api-key path), wizard can use generic profile creation.

## UX Rules
- Wizard auto-opens on first run when setup is incomplete.
- Wizard can complete all required setup without forcing users into separate screens.
- Every input has:
  - short plain-language description
  - example value
  - clear validation error
- Advanced settings are collapsed by default.
- Every step shows explicit status: `Not started`, `Needs attention`, `Ready`, `Completed`.

## Auto-Show Conditions
Show wizard automatically when any of these are true:
- gateway URL missing
- gateway token missing
- agent list empty
- no enabled profile for the selected provider path (only for providers that require profiles)

Reopen entrypoint:
- persistent `Setup Wizard` action in top bar (and optional Cockpit tile)

Dismiss state:
- local key: `mission_control.onboarding.dismissed_at_ms`
- if dismissed < 24h ago, do not auto-open unless setup is broken (connection/auth unusable)

## Flow Model (OpenClaw-Style)

### Step 0: Mode Select
User chooses:
- `Quickstart (Recommended)`
- `Manual`

Quickstart:
- pre-fills local defaults and minimizes decisions
- favors "safe + usable now" over full customization

Manual:
- exposes advanced fields (gateway overrides, provider details)

### Step 1: Preflight
Run non-destructive checks and show status matrix:
- gateway reachable
- token accepted
- role capability for setup actions
- existing agents count
- existing provider profiles count by provider

If token valid but insufficient permissions:
- show read-only-safe guidance
- allow user to continue only with non-privileged actions
- block privileged steps with clear reason (`operator_admin required`)

### Step 2: Gateway Connect
Goal: verified gateway session.

UI:
- gateway URL
- gateway token (password field)
- actions: `Test Connection`, `Save + Connect`

Validation behavior:
- `Test Connection` must include both:
  - reachability probe (`/api/v1/health`)
  - authenticated probe (`/api/v1/status` or equivalent role-gated read)
- distinguish failures:
  - unreachable host
  - unauthorized/forbidden
  - malformed URL

Quickstart default:
- use existing saved URL if present, else `http://127.0.0.1:18789`

### Step 3: Agent
Goal: ensure at least one runnable agent exists.

If agents exist:
- select one and continue

If no agents:
- create form:
  - `agent_id` (suggest `lyra`)
  - `name` (suggest `Lyra`)
  - `workspace_root` (default `.`)
  - `tool_profile` (default `default`)

API:
- `POST /api/v1/agents`

### Step 4: Provider Path
Goal: attach at least one provider path.

User picks:
- `Anthropic (Claude)`
- `OpenAI`
- `Local connector`

#### Anthropic branch
- fields: `display_name`, `setup_token`, optional `api_base_url`
- endpoint: `POST /api/v1/auth/anthropic/setup-token/ingest`
- default `enabled = true`

#### OpenAI branch
- OAuth start -> browser authorize -> OAuth finish
- endpoints:
  - `POST /api/v1/auth/openai/oauth/start`
  - `POST /api/v1/auth/openai/oauth/finish`
- fallback: manual callback code/state path if deep-link handling fails

#### Local connector branch
- no OAuth required
- optional provider profile creation if chosen (API key mode)
- branch can complete without secret ingestion when provider does not require auth profile

### Step 5: Routing
Goal: map chosen agent to chosen provider profile (when required).

Endpoints:
- `GET /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order`
- `POST /api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order`

Safe update algorithm (required):
1. fetch existing `profile_ids`
2. remove target profile id if already present
3. prepend target profile id
4. append previous remaining profile ids in original order
5. write updated order

For provider paths that do not require profile order:
- step reports `not required` and continues

### Step 6: Review + Apply
Show a summary card before final apply:
- gateway target
- selected agent
- selected provider path
- profile created/selected
- routing action preview

Actions:
- `Back`
- `Apply`

### Step 7: Done
Show explicit green check matrix:
- connected
- agent ready
- provider ready
- routing ready (or not required)

Primary CTA:
- `Go to Boards`
Secondary CTA:
- `Open Cockpit`

## API Wrapper Additions (Frontend)
Extend `apps/mission-control/src/lib/api.ts` with:
- `createAgent(settings, payload)` -> `POST /api/v1/agents`
- `ingestAnthropicSetupToken(settings, payload)` -> `POST /api/v1/auth/anthropic/setup-token/ingest`
- `startOpenAiOauth(settings, payload)` -> `POST /api/v1/auth/openai/oauth/start`
- `finishOpenAiOauth(settings, payload)` -> `POST /api/v1/auth/openai/oauth/finish`
- optional generic `createAuthProfile(settings, payload)` -> `POST /api/v1/auth/profiles` (for non-OAuth profile paths)

Reuse existing wrappers:
- `getGatewayHealth`
- `getGatewayStatus`
- `listAgents`
- `listAuthProfiles`
- `getAgentProviderProfileOrder`
- `setAgentProviderProfileOrder`

## Frontend Module Plan
Create:

```text
apps/mission-control/src/features/onboarding/
  OnboardingWizard.tsx
  OnboardingStepShell.tsx
  steps/
    StepMode.tsx
    StepPreflight.tsx
    StepConnect.tsx
    StepAgent.tsx
    StepProvider.tsx
    StepRouting.tsx
    StepReview.tsx
    StepDone.tsx
  useOnboardingController.ts
  onboardingState.ts
```

Integration:
- overlay mounted from app shell level
- auto-show logic based on setup completeness + dismiss-age rules
- explicit reopen trigger in topbar

## Security Requirements
- secret/token fields are always masked inputs
- clear drafts immediately after successful submission
- never write secret values to localStorage
- never include secret values in notices/events/logs
- OAuth callback parsing must validate state and session IDs

## Edge Cases / Failure Modes
- gateway reachable, token rejected -> clear auth error and retry affordance
- user lacks role for create/profile/routing -> step blocked with role message and next actions
- duplicate agent id -> conflict handling with suggestion
- duplicate profile display name -> conflict handling with suggestion
- OAuth exchange failure/timeouts -> retry + manual callback fallback
- setup interrupted midway -> resume from persisted non-secret state

## QA Matrix (Required)

### Fresh state
- no URL/token/agents/profiles
- wizard auto-opens
- quickstart path completes end-to-end
- lands on Boards with success statuses

### Existing state
- fully configured -> wizard does not auto-open
- wizard can reopen manually
- wizard does not overwrite existing routing without review confirmation

### Permissions
- admin token: full flow works
- readonly token: privileged steps blocked with explicit reason

### Provider branches
- Anthropic branch passes create + route
- OpenAI OAuth branch passes start/finish + route
- Local connector branch passes with no OAuth requirement

### Regression
- Mission Control baseline screens still load and operate
- no secret leakage in UI logs/events

## Deliverables
- onboarding wizard overlay in Mission Control
- required API wrappers
- README update documenting:
  - auto-show logic
  - reopen action
  - supported provider paths
  - security handling rules
