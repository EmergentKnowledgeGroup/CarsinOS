# carsinOS Full Audit Remediation Spec

Generated: 2026-05-04
Status: Implemented and validated on 2026-05-04
Source audit: Full mapping/audit/QA review from `FULL_AUDIT_QA_REVIEW WORK`
Execution checklist: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_EXECUTION_CHECKLIST.md`
Blockerboard: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_BLOCKERBOARD.md`

## 1. Purpose

This spec turns the full source audit findings into a concrete remediation plan for the local `carsinos` checkout.

The work has four outcomes:

1. Stop known secret/token exposure paths.
2. Close connector/runtime safety gaps without creating a second execution model.
3. Restore trustworthy QA gates for Rust, Mission Control, acceptance checks, and E2E.
4. Improve operator-facing UX where the current flows hide security state, misrepresent readiness, or rely on ambiguous labels.

This is not a feature expansion pass. It is a hardening and cleanup pass.

## 1.1 Planning Artifact Status

This file is the source spec.

The execution checklist and blockerboard were produced by the `REMEDIATION_SPEC_SWARM WORK` planning lane after SpecSwarm fold-in and before implementation starts. They are locked outputs derived from this spec.

Root `tools/context_checkpoint.py` is absent in this checkout. Until it exists, all required runtime checkpoint updates for this lane are maintained manually in `runtime/checkpoints/LATEST.md`, `runtime/checkpoints/LATEST.json`, and `CHECKPOINT.md`.

## 1.2 Implementation Closeout

This remediation has been implemented in the local checkout and validated with the final matrix recorded in `docs/CARSINOS_FULL_AUDIT_REMEDIATION_EXECUTION_CHECKLIST.md`.

Closed scope:

- WebSocket bearer query auth replaced with authenticated one-time tickets.
- Gateway HTTP trace URI query values redacted.
- Generated gateway token removed from normal log fields/messages.
- Mission Control production browser fallback no longer stores gateway token in `localStorage`.
- Connector import/execution network policy and per-operation auth metadata added.
- Tool binary allowlist rejects path-qualified commands.
- Plugin bundle version/path handling validates and contains filesystem joins.
- Production Tauri CSP removes script inline/eval and remote font/style origins.
- Public-bind gateway mode no longer inherits permissive dev CORS defaults.
- P1 acceptance matrix, PR/release quality gates, Tauri smoke, and Playwright mock gateway were updated to the hardened contracts.
- Mission Control UI copy now distinguishes desktop keyring, browser memory-only runtime storage, E2E session-only storage, and connector auth coverage.

Remaining workflow item:

- Open the PR, request review, address review feedback, re-run gates, and merge. This is tracked as B7 in the execution checklist and is not an implementation blocker.

## 2. Readiness Target

The target state is:

- source findings are fixed or explicitly deferred with a blocker and owner
- Mission Control can typecheck, lint, unit test, build, and run targeted E2E from this Windows/UNC checkout
- Rust workspace tests are green from the UNC checkout using the Z-drive Cargo target/cache
- secrets are not logged, persisted in browser `localStorage`, exported, checkpointed, or shown in routine UI
- connector-derived tools cannot reach disallowed network targets
- auth-required connector behavior is enforceable and visible
- QA evidence is checkpointed after green runs

## 3. Non-Goals

- No wholesale rewrite of `crates/carsinos-gateway/src/main.rs`.
- No new gateway, connector, or tool execution runtime.
- No second frontend state-management framework.
- No permanent compatibility path that keeps both old unsafe behavior and new safe behavior alive.
- No broad UI redesign, hero pages, marketing layout, or decorative visual refresh.
- No hidden fallback that silently bypasses security policy in dev, test, or production.
- No full shared-target delete or `cargo clean` without an explicit checkpoint and user approval.

## 4. Operating Rules

1. Fix the smallest safe surface that proves the issue is closed.
2. Prefer one canonical helper per policy instead of duplicate ad hoc checks.
3. Write failing regression tests before or alongside every security fix.
4. Use existing repo patterns and APIs unless the current pattern is the bug.
5. Keep user-owned dirty worktree changes intact.
6. Update `runtime/checkpoints/LATEST.md`, `runtime/checkpoints/LATEST.json`, and `CHECKPOINT.md` at phase start, post-green tests, PR open, and post-merge.
7. Use track `REMEDIATION_SPEC_SWARM WORK` for this planning lane and a new implementation track when code work begins.
8. When a UI change is required, implementation instructions must explicitly use `frontend-design` and `ui-ux-pro-max-sanitized` before editing UI files.
9. UI remediation must preserve Mission Control's operator-tool feel: dense, scannable, dark-first compatible, plain-English labels, restrained motion, and clear status hierarchy.
10. Any proposal that adds a new abstraction must explain which duplicated or unsafe behavior it removes.

## 5. Required Skills for Implementation

Before UI implementation tasks:

- Use `frontend-design` to pick the specific interaction pattern and avoid generic, bolted-on UI.
- Use `ui-ux-pro-max-sanitized` for a local UX/accessibility/design-system sanity pass.

The local UX brief for this remediation favors a trust-and-authority operator interface: visible safety state, clear prerequisites, accessible focus states, reduced ambiguity, and strong contrast. Do not copy a marketing palette or add visual decoration from that brief; apply the underlying UX principle to the existing Mission Control product language.

Before implementation planning:

- Use `superpowers:writing-plans` or an equivalent checklist format that produces bite-sized tasks with files, commands, and expected results.

Before completion:

- Use `superpowers:verification-before-completion`; do not claim green without fresh command output.

## 6. Finding Remediation Requirements

### R1. WebSocket bearer token leak

Source finding:

- `apps/mission-control/src/lib/api.ts` builds `WS /api/v1/ws?token=<token>`.
- `crates/carsinos-gateway/src/main.rs` accepts `WsAuthQuery.token`.
- `TraceLayer` logs `request.uri()`.

Required remediation:

1. Add a gateway URI redaction helper used by all HTTP tracing.
2. Redact query strings before logging. The trace span may include path, method, and request id, but never query values.
3. Replace the WebSocket bearer-token query contract with a short-lived one-time WebSocket ticket.
4. The ticket endpoint must require normal bearer auth.
5. The WebSocket URL may carry `?ticket=<opaque>` because the ticket is single-use, short TTL, scoped to WebSocket connection, and not equal to the gateway bearer token.
6. Remove `?token=` usage from Mission Control after ticket support lands.
7. Do not keep permanent dual auth. If a temporary compatibility path is needed for E2E, gate it by an explicit dev/E2E flag and delete it in the same remediation milestone.
8. Tickets must use CSPRNG entropy of at least 128 bits.
9. Ticket TTL must be no longer than 60 seconds.
10. Ticket validation and consumption must be atomic.
11. Tickets must be scoped to the authenticated principal and allowed WebSocket roles.
12. Expired tickets must be cleaned up opportunistically without blocking request handling.

Suggested backend shape:

- `POST /api/v1/ws-ticket`
- request auth: existing bearer roles that can open WS
- response: `{ "ticket": "...", "expires_at": 123 }`
- state: in-memory ticket store with hashed ticket values and expiry
- consumption: `ws_handler` accepts `ticket`, validates, consumes, then opens the socket
- protocol structs in `crates/carsinos-protocol/src/lib.rs`, such as `CreateWebSocketTicketResponse`
- deployment target: this remediation targets a single gateway process. Do not add Redis, database-backed ticket storage, or external ticket infrastructure. If multi-instance deployment becomes required, use sticky routing or file a blocker before implementation.

Suggested frontend shape:

- add `createWebSocketTicket(settings)` API wrapper
- change live-feed connection setup to fetch ticket before constructing the WS URL
- keep failure copy plain: "Live feed needs a fresh gateway session. Reconnect."

Primary touchpoints:

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-gateway/tests/common/mod.rs`
- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/lib/ws.ts`

Tests:

- unit test URI redaction strips all query values
- gateway test proves `?token=` is rejected when not in explicit dev compatibility mode
- gateway test proves ticket is single-use
- gateway test proves expired ticket is rejected
- gateway test proves two concurrent validations of one ticket allow only one connection
- frontend test proves WS URL builder does not include gateway token
- update or delete tests/helpers named like `connect_ws_with_query_token`; do not preserve a permanent `?token=` test lane

Stop-ship:

- any normal log line can contain the bearer token
- any frontend production path still builds `?token=`

### R2. Generated gateway token is logged

Source finding:

- `warn!(token = %config.token, "CARSINOS_GATEWAY_TOKEN not set; generated runtime token")`

Required remediation:

1. Remove the token from tracing fields and log messages.
2. Replace with a non-secret warning that explains how the operator should configure `CARSINOS_GATEWAY_TOKEN`.
3. If one-click launch or local bootstrap needs the generated token, deliver it through the existing one-click environment/keyring handoff path outside normal logs.
4. The handoff must be excluded from checkpoints and security reports.
5. Update docs/scripts that currently rely on reading the token from gateway logs.

Canonical handoff for this remediation:

- use the existing Mission Control/Tauri keyring path where the desktop app is available
- use one-click launch environment propagation for local bootstrap where needed
- do not add a new account system, repo-local secret file, or second permanent secret store

Do not invent a full account system for this fix.

Primary touchpoints:

- `crates/carsinos-gateway/src/main.rs`
- `apps/mission-control/src-tauri/src/lib.rs`
- `scripts/one_click_launch.sh`
- `scripts/one_click_launch.command`

Tests:

- unit or integration test captures tracing output and verifies generated token is absent
- hardcoded-value guard still passes
- launch-script smoke test or documented manual check verifies local setup can still obtain a usable token

Stop-ship:

- generated token appears in gateway log file, stdout tracing, security report, checkpoint, or browser state

### R3. Connector execution lacks SSRF enforcement

Source finding:

- `prepare_connector_import` stores `endpoint_url`.
- `execute_openapi_connector_call`, `execute_graphql_connector_call`, and `execute_mcp_connector_call` parse and call that URL directly.
- Connector spec requires gateway-owned SSRF protections.

Required remediation:

1. Add one canonical connector network policy helper in gateway code or a focused gateway module.
2. Use the helper at import time and at execution time.
3. Allow only `https` by default.
4. Allow `http` only for explicit local development allowlist entries.
5. Reject loopback, private, link-local, multicast, unspecified, documentation, and metadata-service ranges.
6. Resolve DNS and reject if any resolved address is disallowed.
7. Re-check policy at execution time to reduce stale import and DNS rebinding risk.
8. Use a connector-specific HTTP client or per-request redirect behavior so provider model fetching remains unchanged.
9. Disable automatic redirects for connector execution in the first implementation.
10. If redirects are later allowed, validate every hop before forwarding credentials, body, or query auth.
11. Metadata-service denies override every allowlist.
12. Make policy failures visible in connector health/review without exposing sensitive URL details.
13. Keep policy configuration small: env allowlist for hostnames/CIDRs is acceptable; a new policy engine is not.

DNS and connect rules:

- resolve the host before execution
- reject if any resolved address is disallowed
- connect only through the validated target path or fail closed
- re-run validation on every execution
- do not forward connector auth until the final request target has passed policy
- connect only to the validated address/path. If the current HTTP stack cannot pin the validated address without re-resolution, implementation must record that as a blocker instead of shipping a validate-then-re-resolve path.

Allowlist semantics:

- env names:
  - `CARSINOS_CONNECTOR_ALLOW_HTTP_HOSTS`
  - `CARSINOS_CONNECTOR_ALLOW_PRIVATE_CIDRS`
- allowlist entries are exact hostnames or CIDRs, comma-separated
- wildcard hostnames are not allowed in this remediation
- metadata-service denies always win over every allowlist
- private CIDR allowlist is development-only unless explicitly accepted on the blockerboard

Minimum blocked targets:

- `127.0.0.0/8`
- `::1`
- `10.0.0.0/8`
- `172.16.0.0/12`
- `192.168.0.0/16`
- `169.254.0.0/16`
- `fc00::/7`
- `fe80::/10`
- `0.0.0.0/8`
- cloud metadata hostnames and IPs such as `169.254.169.254`

Primary touchpoints:

- `crates/carsinos-gateway/src/main.rs`
- optional focused module `crates/carsinos-gateway/src/connector_network_policy.rs` only if the current gateway module layout supports it without route churn
- connector import and execution tests in gateway test modules

Tests:

- import rejects private/loopback/link-local URLs
- execution rejects a stored URL that becomes disallowed
- redirect to disallowed target fails closed
- redirect does not leak auth headers, query auth, or request body to the redirected target
- dev allowlist permits only the explicit configured local target
- health/review surfaces show a safe degraded reason

Stop-ship:

- a connector-derived tool can call an arbitrary private/internal endpoint by imported URL

### R4. Connector auth-required gate is effectively dead

Source finding:

- `connector_auth_required` reads `import_metadata_json["auth_required"]`.
- `ImportConnectorRequest` has no `auth_required` field.
- `prepare_connector_import` never writes `auth_required`.

Required remediation:

1. Do not trust a frontend-supplied `auth_required` boolean as the canonical source.
2. Derive auth state per operation/tool, not only per connector:
   - OpenAPI security schemes and operation security
   - GraphQL connector metadata
   - MCP connector metadata/tool annotations where available
3. Store the first implementation in `import_metadata_json` / origin metadata to avoid an unnecessary migration unless implementation proves a dedicated column is required.
4. Expose per-operation auth state in protocol responses and aggregate connector health.
5. Treat ambiguous metadata as review-required/degraded and fail closed at execution.
6. Ensure connector execution blocks and creates auth-repair interaction when required auth is missing.
7. Ensure health correctly reports auth-required degraded state.

Canonical auth-state contract:

- operation key: stable connector operation identity used by conversion and published-tool origin metadata
- state enum:
  - `none`
  - `required`
  - `ambiguous_review_required`
- source enum:
  - `openapi_security`
  - `graphql_metadata`
  - `mcp_metadata`
  - `operator_review`
  - `unknown`
- reason: short non-secret string suitable for review/health UI
- aggregate `auth_required`: true if any currently published operation has state `required` or `ambiguous_review_required`
- aggregate degraded state: true when an assigned/enabled connector has any callable operation with `required` and no applicable binding, or any operation with `ambiguous_review_required`
- binding precedence: per-agent binding wins, then shared binding, then no binding
- execution behavior: `required` without binding creates auth interaction and fails closed; `ambiguous_review_required` fails closed until operator review resolves it

Primary touchpoints:

- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-storage/src/lib.rs` only if existing metadata storage cannot represent the derived state safely

Tests:

- source with OpenAPI security scheme sets auth required
- mixed OpenAPI source preserves public and secured operation-level auth differences
- source with no security remains auth optional
- ambiguous auth metadata becomes review-required/degraded or execution fail-closed
- execution without required auth creates auth interaction and does not call remote endpoint
- shared auth satisfies requirement
- per-agent override wins over shared auth
- health changes when auth binding is added/removed

Stop-ship:

- connector with required auth can execute without binding or interaction

### R5. Gateway token contract is violated in browser localStorage fallback

Source finding:

- `API_CONTRACT.md` says gateway tokens must never be stored in localStorage.
- `runtime.ts` stores `TOKEN_KEY_FALLBACK` in localStorage for non-Tauri runtime.

Required remediation:

1. Remove gateway token persistence from `localStorage`.
2. Keep Tauri keyring behavior.
3. Browser/dev canonical path is in-memory per-tab token after operator entry or environment token when configured.
4. E2E-only exception: `sessionStorage` may be used only when explicit flag `VITE_CARSINOS_E2E_TOKEN_STORAGE=1` is set.
5. Precedence: Tauri keyring, then environment token, then in-memory token, then E2E-only sessionStorage.
6. Refresh behavior: browser in-memory token is lost on reload and the operator must reconnect unless env token is present.
7. Make the storage mode visible in setup/debug copy without showing the token.
8. Update E2E helpers to seed tokens through the approved dev/E2E path.
9. Update `API_CONTRACT.md` to match the implemented behavior.
10. On startup, delete and ignore the legacy `gatewayTokenFallback` localStorage key.
11. The cleanup must be silent unless it affects the current session, in which case the UI should ask the operator to reconnect.

UX requirements:

- The setup screen should explain whether the token is saved securely, kept for this session, or supplied by environment.
- Use plain text such as "Saved in the desktop keychain" or "Kept only for this browser session."
- Do not add scary generic security language. Be precise.

Tests:

- unit test proves `localStorage.setItem(gatewayTokenFallback, ...)` is no longer used
- unit test seeds legacy localStorage and proves startup purges it
- E2E helper works through dev/E2E mode
- Tauri path still invokes keyring commands
- browser refresh behavior is intentional and documented

Primary touchpoints:

- `apps/mission-control/src/lib/runtime.ts`
- `apps/mission-control/src/lib/runtime.test.ts`
- `apps/mission-control/e2e/onboardingFlow.ts`
- `apps/mission-control/API_CONTRACT.md`

Stop-ship:

- gateway token is stored in `localStorage` in any production path

### R6. Tauri CSP allows inline and eval scripts plus remote fonts

Source finding:

- `script-src 'unsafe-eval' 'unsafe-inline'`
- remote Google font/style origins

Required remediation:

1. Remove `unsafe-eval` and `unsafe-inline` from production Tauri CSP.
2. Bundle fonts or use the existing local/system font stack.
3. Keep `connect-src` limited to local gateway and approved carsinos origins.
4. If dev requires looser CSP, isolate it to dev-only config and keep production strict.
5. Verify Tauri build/runtime still renders.

Tests:

- static config assertion for no production `unsafe-eval`
- static config assertion for no production `unsafe-inline` in `script-src`
- static config assertion for no remote font/style origins in production
- static CSP assertion uses an allowlist for production directives and rejects broad wildcards
- Mission Control build passes
- Tauri smoke test or documented manual run confirms app boot

Primary touchpoints:

- `apps/mission-control/src-tauri/tauri.conf.json`
- `apps/mission-control/src/styles.css`
- `apps/mission-control/src/app/useTheme.ts`

Stop-ship:

- production desktop app needs inline/eval script CSP

### R7. Tool binary allowlist can be bypassed by path-qualified binaries

Source finding:

- `ensure_binary_allowed` normalizes by `file_name`.
- `Command::new(&binary)` executes the original path.

Required remediation:

1. Reject path-qualified binaries unless a separate absolute-binary allowlist is explicitly configured.
2. Default allowlist entries are bare command names only.
3. Keep `parse_exec_command` shell-operator rejection intact.
4. If absolute binaries are supported later, canonicalize them and compare against canonical allowed paths.
5. Do not add a shell wrapper.
6. On Windows, sanitize PATH/PATHEXT resolution or resolve bare binaries only from trusted command directories.
7. Reject current-working-directory command resolution for allowlisted bare names.
8. Do not allow `git.cmd`, `git.bat`, or similarly named wrappers to satisfy an allowlist entry unless that exact extension/path is explicitly trusted.

Primary touchpoints:

- `crates/carsinos-tools/src/lib.rs`

Tests:

- `git status` or another bare allowlisted command still works
- `C:\temp\git.exe status` is rejected by default
- `..\git.exe status` is rejected by default
- Unix-style `/tmp/git status` is rejected by default
- cwd-local `git.exe`, `git.cmd`, or `git.bat` does not hijack an allowlisted `git`
- PATH entry pointing at an untrusted directory does not satisfy the allowlist
- error message tells the operator that path-qualified binaries are not allowed

Stop-ship:

- caller can execute an arbitrary path by naming the file after an allowlisted binary

### R8. Plugin version path traversal risk

Source finding:

- `plugin_bundle_dir(...).join(plugin_version)`
- core validation only checks non-empty `plugin_version`.

Required remediation:

1. Add a single plugin version normalization/validation helper in `carsinos-core`.
2. Allow only a safe identifier/semver-like set: ASCII alnum plus `.`, `_`, `-`, `+`.
3. Reject path separators, drive prefixes, `.` and `..`, empty segments, absolute paths, and control characters.
4. Apply the normalized value before storage and before gateway filesystem joins.
5. Add a defense-in-depth containment check after bundle path construction.
6. Reject Windows reserved device names, trailing dots/spaces, and path segments over 128 characters.
7. Verify plugin IDs used in bundle paths go through equivalent safe identifier normalization.
8. Resolve containment after canonicalization where possible and guard against symlink/junction escapes.

Containment algorithm:

- validate plugin ID and version components before join
- canonicalize the plugin storage root before writes
- canonicalize the existing parent directory before each write
- require the canonical parent to remain inside the canonical storage root
- after creating directories/files, canonicalize the created parent/final path and re-check containment
- reject symlink or junction traversal that escapes the storage root

Primary touchpoints:

- `crates/carsinos-core/src/lib.rs`
- `crates/carsinos-gateway/src/main.rs`

Tests:

- valid `1.0.0`, `1.0.0-beta.1`, and `2026.05.04+local` accepted
- `../x`, `..\x`, `C:\x`, `/tmp/x`, `.`, `..`, and `1/2` rejected
- `CON`, `NUL`, `COM1`, trailing-dot, trailing-space, and overlong versions rejected on Windows
- materialized plugin bundle path remains inside plugin storage root

Stop-ship:

- plugin install/update can write outside the intended plugin storage root

### R9. P1 acceptance matrix stale after E2E helper extraction

Source finding:

- matrix still asserts strings inside `apps/mission-control/e2e/core.spec.ts`
- strings now live in `apps/mission-control/e2e/onboardingFlow.ts`

Required remediation:

1. Update the acceptance matrix to track the actual source of truth.
2. Prefer semantic assertions over brittle exact file/string checks where possible.
3. If the script remains static, it must support helper file references.
4. Add a test for the acceptance checker itself or a documented dry-run command.
5. Define one acceptance manifest/checker contract; do not create parallel assertion formats.

Primary touchpoints:

- `scripts/mission_control_phase_acceptance_check.py`
- `docs/mission-control_p1_acceptance_matrix.json`
- `apps/mission-control/e2e/onboardingFlow.ts`
- `scripts/tests/` for checker fixtures/tests

Tests:

- `python scripts/mission_control_phase_acceptance_check.py --phase P1` passes after update
- moving a required helper assertion out of the tracked files fails the checker
- positive and negative checker fixtures cover helper-file assertions

Stop-ship:

- a required release matrix can fail due to known false negatives

## 7. QA Infrastructure Remediation

### Q1. Mission Control dependency recovery

Current blocker:

- Vite and Vitest fail because `@rollup/rollup-win32-x64-msvc` is missing.

Required remediation:

1. Run dependency repair from `apps/mission-control`.
2. Prefer `npm install` first because it should restore optional dependencies without deleting local work.
3. Do not delete `node_modules` or lockfiles without checkpoint and explicit user approval.
4. Re-run:
   - `node .\node_modules\vitest\vitest.mjs run`
   - `node .\node_modules\vite\bin\vite.js build`
   - package scripts if UNC-safe

Exit criteria:

- `npm ls @rollup/rollup-win32-x64-msvc` shows installed package
- Vitest and Vite build no longer fail on Rollup native package load

### Q2. Playwright UNC webServer recovery

Current blocker:

- Playwright webServer starts from UNC cwd; CMD falls back to `C:\Windows`; relative `e2e/mockGateway.mjs` cannot be found.

Required remediation:

1. Make Playwright webServer commands UNC-safe.
2. Prefer absolute paths or a PowerShell launcher that sets UNC cwd correctly.
3. Avoid `npm run dev` inside Playwright webServer if command resolution is unstable from UNC.
4. Use direct `node node_modules/vite/bin/vite.js` invocation if needed.
5. Keep ports 1420 and 19789 unless the config already makes them overridable.
6. Do not allow release gates to reuse stale servers on ports 1420 or 19789.
7. If server reuse is needed for local development, require a health marker tied to the current checkout and mock gateway.
8. Prefer `process.execPath` and absolute paths for Node launch commands.

Primary touchpoints:

- `apps/mission-control/playwright.config.ts`
- `apps/mission-control/e2e/mockGateway.mjs`

Tests:

- `node .\node_modules\@playwright\test\cli.js test --config=playwright.config.ts --grep "@core"` reaches test execution instead of webServer startup failure
- targeted core test passes after app/dependency blockers are resolved

### Q3. Rust shared-target/toolchain recovery

Current blocker:

- `cargo test --workspace --locked` fails compiling dependencies from the shared UNC target/toolchain state.

Required remediation:

1. Preserve `.cargo/config.toml` shared target.
2. Verify toolchain version and target directory before cleanup.
3. Retry from the same UNC checkout after no parallel Cargo jobs are running.
4. If still failing, try the same checkout via a mapped drive path while keeping the shared target.
5. Treat mapped-drive success as diagnostic only; final green must run from the canonical UNC checkout unless the user explicitly accepts a documented UNC/toolchain blocker.
6. If artifact corruption remains likely, do targeted Cargo cleanup for the failing dependency artifacts only after checkpoint and user approval.
7. Do not run whole-workspace destructive cleanup by default.

Exit criteria:

- `cargo test --workspace --locked` passes, or a precise external/toolchain blocker is documented with attempted commands and next recovery step

## 8. Secondary Cleanup and Design-Debt Requirements

### D1. Gateway file concentration

`crates/carsinos-gateway/src/main.rs` is too large to keep absorbing every remediation.

Requirement:

- Do not attempt a broad split.
- When adding new policy helpers, prefer focused modules only if the file structure already supports it cleanly.
- Acceptable extraction candidates:
  - `gateway/src/redaction.rs`
  - `gateway/src/connector_network_policy.rs`
  - `gateway/src/ws_ticket.rs`
- Each extracted module must have tests and a narrow public API.

Anti-overengineering check:

- If extraction needs sweeping route rewiring, keep the helper local and defer structural cleanup.

### D2. CORS default looseness

Current risk:

- `CARSINOS_ENV` unset defaults to dev CORS with `Any` origin/method/header.

Requirement:

- This is promoted into the remediation scope.
- Production or public-bind mode must not silently get permissive CORS.
- Keep it small: `CARSINOS_ENV` unset must not imply permissive CORS when public bind is enabled.
- Public-bind releases with permissive CORS are stop-ship.
- Do not add a new CORS policy framework; adjust the default decision and tests around the existing helper.
- Consume the existing gateway config/network exposure decision input rather than duplicating unrelated env parsing.

Primary touchpoints:

- `crates/carsinos-gateway/src/main.rs`

Tests:

- unset `CARSINOS_ENV` with loopback bind keeps local development behavior
- unset `CARSINOS_ENV` with public bind does not allow `Any` origin
- explicit dev/local env keeps permissive CORS only for local development

### D3. Anthropic validation truthfulness

Current risk:

- Some UI copy may imply a remote token/model validation when the backend only checks shape or partial readiness.

Requirement:

- Audit onboarding/provider validation copy.
- Replace "models loaded" claims with exact truth, such as "Token format accepted" or "Provider responded with models."
- Do not add extra provider calls unless needed to make the claim true.

### D4. UI display and flow cleanup

Required UI improvements are limited to security/readiness clarity:

- token storage mode display
- live feed reconnect/degraded state
- connector health/auth-required state
- acceptance/QA failure visibility if exposed in Runbook or Help
- CSP/font changes must not visually regress text readability

Implementation instructions:

- Use `frontend-design` before editing UI files.
- Use `ui-ux-pro-max-sanitized` before editing UI files.
- Preserve existing Mission Control layout density and navigation.
- Use status badges, inline helper text, disabled affordances with reasons, and focused empty/degraded states.
- Critical security/readiness state must be visible text, not tooltip-only.
- Do not create cards inside cards.
- Do not add marketing hero sections, decorative gradients, or broad theme changes.
- Prefer icon plus tooltip for technical status controls where the app already uses that pattern.
- Test at 375, 768, 1024, and 1440 widths if visible UI changes are made.

## 9. Implementation Order

### Phase 0. Planning artifacts and minimum gate recovery

1. Write or refresh the execution checklist and blockerboard from this locked spec.
2. Restore dependency/tooling prerequisites enough to run minimum tests.
3. Minimum gates before security edits:
   - `cargo fmt --all -- --check`
   - Mission Control typecheck
   - Mission Control lint
   - security hardcoded-value guard
   - targeted unit tests for the file being changed, if the relevant harness is available
4. Add focused failing tests for each security finding before implementation where practical.
5. Write phase-start checkpoint.

### Phase 1. Token and log hardening

1. URI redaction in gateway traces.
2. Generated token log removal.
3. WebSocket one-time ticket flow.
4. Mission Control WS ticket client update.
5. Token storage fallback cleanup.

### Phase 2. Connector and execution safety

1. Connector network policy helper.
2. Import-time and execution-time SSRF enforcement.
3. Connector auth-required derivation and enforcement.
4. Connector health/auth UI copy if affected.

### Phase 3. Containment hardening

1. Tool binary path-qualified allowlist rejection.
2. Plugin version validation and bundle containment.
3. CSP tightening and font/local asset cleanup.

### Phase 4. Full QA harness recovery

1. P1 acceptance matrix repair.
2. Playwright UNC-safe webServer command.
3. Full frontend gate run.
4. Full Rust gate run or documented external blocker.

### Phase 5. Final cleanup and docs

1. Update API contracts, README/security docs, and runbooks.
2. Update blockerboard statuses.
3. Write post-green checkpoints.
4. Open PR only after local gates are green or blockers are explicitly accepted.

## 10. Required Verification Matrix

Run from repo root unless noted.

| Gate | Command | Required Result |
| --- | --- | --- |
| Rust format | `cargo fmt --all -- --check` | PASS |
| Rust tests | `cargo test --workspace --locked` | PASS or documented external/toolchain blocker |
| Mission Control typecheck | `node .\node_modules\typescript\bin\tsc -b --pretty false` in `apps/mission-control` | PASS |
| Mission Control lint | `node .\node_modules\eslint\bin\eslint.js .` in `apps/mission-control` | PASS |
| Mission Control unit | `node .\node_modules\vitest\vitest.mjs run` in `apps/mission-control` | PASS |
| Mission Control build | `node .\node_modules\vite\bin\vite.js build` in `apps/mission-control` | PASS |
| P1 acceptance | `python scripts/mission_control_phase_acceptance_check.py --phase P1` | PASS |
| Security guard | `python scripts/security_hardcoded_value_guard.py` | PASS |
| Core E2E | `node .\node_modules\@playwright\test\cli.js test --config=playwright.config.ts --grep "@core"` in `apps/mission-control` | PASS |

## 11. Stop-Ship List

Do not open a release PR if any of these are true:

- gateway token appears in logs, localStorage, checkpoints, exported packages, or routine UI
- WebSocket production path uses bearer token query auth
- connector tool execution can hit private/internal network targets by imported endpoint URL
- connector auth-required source can execute without auth binding or durable interaction
- production Tauri CSP needs `unsafe-eval` or script `unsafe-inline`
- public-bind gateway mode can run with permissive default CORS
- path-qualified binaries pass the default tool allowlist
- cwd/PATH/PATHEXT hijack can satisfy a default tool allowlist entry
- plugin version can influence filesystem paths outside the bundle root
- P1 acceptance matrix is known stale
- Vite/Vitest build gates fail due missing optional dependency
- Playwright cannot start E2E web servers from this local checkout

## 11.1 Rollout and Backout

### Token and WebSocket rollout

- preflight: Mission Control can fetch gateway settings and normal bearer API calls succeed
- expected breakage: old browser sessions with localStorage tokens must reconnect
- allowed backout: revert the WS ticket/client commit as a unit before release
- forbidden backout: re-enable production `?token=` bearer query auth or token logging

### Connector safety rollout

- preflight: connector import/execution tests cover allowed public HTTPS and blocked private targets
- expected breakage: previously imported private/local connector endpoints may become degraded
- allowed backout: disable connector-derived execution or mark affected connectors degraded
- forbidden backout: bypass SSRF policy for all connectors or forward auth before policy validation

### Token storage cleanup rollout

- preflight: setup/onboarding explains session-only browser token behavior
- expected breakage: browser reload requires reconnect when no env token exists
- allowed backout: E2E-only sessionStorage flag for tests
- forbidden backout: restore production localStorage persistence

### CSP/CORS rollout

- preflight: production CSP/static assertions pass and public-bind CORS tests pass
- expected breakage: remote font loading and any inline/eval script dependency fail
- allowed backout: fix local assets or dev-only CSP config
- forbidden backout: production `unsafe-eval`, production script `unsafe-inline`, or public-bind permissive CORS

### QA harness rollout

- preflight: dependency repair and checker fixtures pass
- expected breakage: stale servers on test ports fail instead of being reused
- allowed backout: local-dev-only reuse with current-checkout health marker
- forbidden backout: release gates reusing unknown stale servers

## 12. SpecSwarm Review Instructions

When running SpecSwarm on this spec, reviewers must check for:

- missing security edge cases
- unclear file touchpoints
- implementation steps that create duplicate paths or over-engineered frameworks
- UI/UX ambiguity around token storage, connector auth, degraded states, and readiness
- QA gaps that could let false green status through
- Windows/UNC-specific command failures
- places where a minimal fix is safer than a broad refactor

Reviewers should explicitly flag any section that asks for:

- a new runtime/control plane
- a broad gateway rewrite
- permanent dual auth behavior
- a new policy engine where a small helper is enough
- frontend redesign unrelated to security/readiness clarity

## 13. Lock Criteria

This spec is locked when:

1. SpecSwarm gap review, implementation mapping, and final QA have completed.
2. SpecSwarm findings are folded into this file or explicitly rejected with reason.
3. The execution checklist and blockerboard exist and point back to this spec.
4. A final parent QA pass confirms no placeholder, contradiction, or over-engineering requirement remains.

Lock result: complete on 2026-05-04. The spec, execution checklist, and blockerboard exist and cross-link.

Deferral schema:

- blocker id
- owner
- accepted_by
- reason
- affected gate
- release_pr_allowed: yes/no
- expiry or revisit condition
- next command/evidence needed

## 14. SpecSwarm Fold-In Log

SpecSwarm gap review and implementation mapping were completed on 2026-05-04.

Folded changes:

- added planning-artifact status and manual-checkpoint ownership because the checklist/blockerboard did not exist before fold-in
- tightened WebSocket ticket entropy, TTL, atomic consume, principal scope, protocol structs, and query-token test deletion requirements
- chose existing one-click/keyring token handoff instead of a repo-local secret file or second secret store
- strengthened connector SSRF requirements for DNS rebinding, redirect credential leakage, metadata-service denies, and provider-client isolation
- changed connector auth from one connector boolean to per-operation auth state with aggregate health
- added legacy localStorage token purge
- added CSP allowlist/wildcard assertions
- expanded tool binary remediation to cover Windows PATH/PATHEXT and cwd hijack
- expanded plugin path validation to cover Windows reserved names, length caps, plugin ID parity, and symlink/junction containment
- added acceptance-checker fixture requirements
- added stale-server protection for Playwright E2E
- clarified mapped-drive Rust runs are diagnostic only
- promoted permissive CORS default from follow-up risk into remediation scope
- split minimum gate recovery from full QA harness recovery
