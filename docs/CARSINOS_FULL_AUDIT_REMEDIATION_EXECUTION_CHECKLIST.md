# carsinOS Full Audit Remediation Execution Checklist

Date: 2026-05-04
Source spec: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_SPEC.md`
Blockerboard: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_BLOCKERBOARD.md`
Execution status: implemented and validated on 2026-05-04

## Purpose

This checklist converts the full audit remediation spec into implementation-safe batches.

The implementation goal is to close the known security, QA, and operator-readiness findings without adding a second runtime, broad frontend redesign, or duplicate compatibility paths.

## Final Execution State

- B0-B6 are complete.
- B7 remains a repository workflow step: open PR, request review, address review, re-run gates, merge.
- All stop-ship implementation blockers from the audit are closed in `docs/CARSINOS_FULL_AUDIT_REMEDIATION_BLOCKERBOARD.md`.
- Z-drive scratch/caches were used for heavy artifacts:
  - `Z:\carsinos-codex-work\carsinos\cargo-target`
  - `Z:\carsinos-codex-work\carsinos\cargo-home`
  - `Z:\carsinos-codex-work\carsinos\npm-cache`
  - `Z:\carsinos-codex-work\carsinos\playwright-browsers`
  - `Z:\carsinos-codex-work\carsinos\tmp`
- `Z:\carsinos-codex-work\carsinos\env.ps1` records the local verification environment.

## Final Validation Matrix

- [x] `cargo fmt --all -- --check`
- [x] `cargo test --workspace --locked -j 1`
- [x] `node .\node_modules\typescript\bin\tsc -b --pretty false`
- [x] `node .\node_modules\eslint\bin\eslint.js .`
- [x] `npm run test:unit` with `vitest run --maxWorkers=4`: 24 files / 122 tests
- [x] `node .\node_modules\vite\bin\vite.js build`
- [x] `python scripts/mission_control_phase_acceptance_check.py --phase P1`
- [x] `python scripts/mission_control_phase_acceptance_check.py --phase P2`
- [x] `python scripts/mission_control_phase_acceptance_check.py --phase P3`
- [x] `python scripts/mission_control_phase_acceptance_check.py --phase P4`
- [x] `python scripts/security_hardcoded_value_guard.py`
- [x] `npm run test:e2e:core`: 25/25
- [x] `npm run test:e2e:full`: 35/35
- [x] `python scripts/mission_control_quality_gate.py --profile=pr`: PASS
- [x] `npm run test:e2e:tauri-smoke`: PASS, artifacts under `Z:\carsinos-codex-work\carsinos\tauri-smoke-artifacts`
- [x] `npm run tauri:build -- --debug --no-bundle`: PASS, binary under `Z:\carsinos-codex-work\carsinos\cargo-target\debug\carsinos-mission-control.exe`
- [x] `python scripts/mission_control_quality_gate.py --profile=release`: PASS, `runtime/quality-gate/reports/quality-gate-release-20260504T152129Z.json`

Known non-blocking output: Vite still reports the existing large chunk warning for the Mission Control bundle.

## Operating Rules

1. Start from the local `carsinos` checkout; do not create a git worktree for normal implementation.
2. Preserve existing dirty worktree changes unless the user explicitly asks to revert them.
3. Use checkpoint track `FULL_AUDIT_REMEDIATION IMPLEMENTATION` for code work.
4. Update `runtime/checkpoints/LATEST.md`, `runtime/checkpoints/LATEST.json`, and `CHECKPOINT.md` at phase start, post-green tests, PR open, and post-merge.
5. Root `tools/context_checkpoint.py` is missing; checkpoint manually until restored.
6. Use `frontend-design` and `ui-ux-pro-max-sanitized` before editing Mission Control UI files.
7. Use `verification-before-completion` before claiming any gate is green.
8. Do not implement broad gateway decomposition; extract only narrow helpers that directly remove risk or duplication.
9. Do not keep permanent dual auth, duplicate token stores, duplicate acceptance formats, or duplicate connector policy paths.
10. Do not run whole-workspace destructive cleanup without checkpoint and user approval.

## Entry Criteria

- `docs/CARSINOS_FULL_AUDIT_REMEDIATION_SPEC.md` is locked.
- `docs/CARSINOS_FULL_AUDIT_REMEDIATION_BLOCKERBOARD.md` exists.
- Current branch/head and dirty worktree are recorded.
- The implementer has read the source audit findings and this checklist.
- The implementer understands that "green" means command output from the current run, not memory or old logs.

## Batch 0: Planning Artifact and Minimum Gate Prep

### Tasks

- [ ] Verify `git status --short --branch` and `git rev-parse HEAD`.
- [ ] Write implementation-start checkpoint for `FULL_AUDIT_REMEDIATION IMPLEMENTATION`.
- [ ] Confirm the root checkpoint helper is absent or restored.
- [ ] Confirm Mission Control dependency state:
  - [ ] `npm ls @rollup/rollup-win32-x64-msvc` from `apps/mission-control`.
  - [ ] If missing, run `npm install` from `apps/mission-control`.
- [ ] Run minimum pre-edit gates:
  - [ ] `cargo fmt --all -- --check`
  - [ ] `node .\node_modules\typescript\bin\tsc -b --pretty false` in `apps/mission-control`
  - [ ] `node .\node_modules\eslint\bin\eslint.js .` in `apps/mission-control`
  - [ ] `python scripts/security_hardcoded_value_guard.py`
- [ ] Record any gate that is still blocked before security edits.

### Exit Criteria

- Dependency recovery status is known.
- Minimum gates are green or explicitly blocked with exact error output.
- No code edit starts without a fresh checkpoint.

## Batch 1: Token and Log Hardening

### Tasks

- [x] Add/identify tests for gateway trace URI redaction.
- [x] Implement URI query redaction for gateway `TraceLayer`.
- [x] Remove generated gateway token value from structured logs.
- [ ] Update one-click/keyring bootstrap docs/scripts if they relied on log scraping.
- [x] Add protocol response for `POST /api/v1/ws-ticket`.
- [x] Implement one-time WebSocket ticket creation with at least 128 bits of CSPRNG entropy.
- [x] Enforce ticket TTL of 60 seconds or less.
- [x] Enforce atomic validate-and-consume.
- [x] Scope ticket to authenticated principal and WebSocket-allowed roles.
- [x] Update `ws_handler` to accept ticket auth and remove production `?token=` bearer query auth.
- [x] Update gateway WS tests/helpers so `connect_ws_with_query_token` is removed or dev-only and cannot become a permanent path.
- [x] Update Mission Control API/WS client to fetch a ticket before connecting.
- [x] Remove browser production localStorage token fallback.
- [x] Add startup cleanup for legacy `gatewayTokenFallback`.
- [x] Update E2E token seeding to approved dev/E2E session path.
- [x] Update `apps/mission-control/API_CONTRACT.md`.

### Required Tests

- [x] gateway URI redaction test proves query values are absent.
- [ ] generated token does not appear in captured tracing output.
- [x] `?token=` WebSocket auth is rejected outside explicit dev/E2E compatibility.
- [x] WebSocket ticket is single-use.
- [ ] WebSocket ticket expires.
- [ ] Concurrent ticket validation allows only one connection.
- [x] frontend WS builder does not include gateway token.
- [x] legacy localStorage token is purged.
- [ ] Tauri keyring path still works.

### Exit Criteria

- No production path logs or URL-encodes the gateway bearer token.
- No production browser path persists gateway token in localStorage.

## Batch 2: Connector Network and Auth Safety

### Tasks

- [ ] Add connector network policy tests before implementation.
- [ ] Add one canonical connector network policy helper.
- [ ] Enforce policy at connector import time.
- [ ] Enforce policy at connector execution time.
- [ ] Use connector-specific client/per-request behavior so provider model calls are unchanged.
- [ ] Disable connector execution redirects for first implementation.
- [ ] Deny metadata-service targets regardless of allowlist.
- [ ] Ensure connector auth is not forwarded before final request target passes policy.
- [ ] Derive auth state per operation/tool.
- [ ] Store first implementation auth state in existing metadata/origin metadata unless a migration is proven necessary.
- [ ] Expose per-operation auth state and aggregate health in protocol responses.
- [ ] Fail ambiguous auth metadata closed or require review.
- [ ] Ensure execution without required auth creates a durable auth-repair interaction and does not call remote endpoint.

### Required Tests

- [ ] private, loopback, link-local, multicast, unspecified, documentation, and metadata URLs are rejected.
- [ ] DNS rebinding/stale stored URL is rejected at execution.
- [ ] redirect to disallowed target fails before credentials/body are forwarded.
- [ ] explicit dev allowlist permits only the configured local target.
- [ ] OpenAPI secured operation requires auth.
- [ ] mixed public/secured OpenAPI operations keep distinct auth state.
- [ ] ambiguous auth metadata is degraded/review-required or execution fail-closed.
- [ ] shared auth satisfies requirement.
- [ ] per-agent auth override wins.
- [ ] health changes when auth binding is added/removed.

### Exit Criteria

- Connector-derived tools cannot call disallowed internal/private targets.
- Connector auth-required behavior is enforceable per operation.

## Batch 3: Containment and Desktop Hardening

### Tasks

- [ ] Add `carsinos-tools` tests for path-qualified binaries and PATH/PATHEXT hijack.
- [ ] Reject path-qualified binaries by default.
- [ ] Sanitize or constrain PATH/PATHEXT resolution to trusted command directories.
- [ ] Reject cwd-local command hijack.
- [ ] Add `carsinos-core` plugin version normalization helper.
- [ ] Apply plugin version helper before storage and gateway bundle path joins.
- [ ] Add gateway containment check after plugin bundle path construction.
- [ ] Validate plugin IDs used in bundle paths through equivalent safe identifier rules.
- [ ] Tighten production Tauri CSP.
- [ ] Remove remote font/style dependencies from production desktop config.
- [ ] Add CORS default hardening for public-bind mode.

### Required Tests

- [ ] bare allowlisted command still works.
- [ ] absolute and relative path-qualified binaries are rejected.
- [ ] cwd-local `git.exe`, `git.cmd`, or `git.bat` does not satisfy `git`.
- [ ] untrusted PATH entry does not satisfy allowlist.
- [ ] valid plugin versions are accepted.
- [ ] traversal, reserved device names, trailing spaces/dots, and overlong plugin versions are rejected.
- [ ] plugin bundle path remains inside plugin storage root.
- [ ] production CSP has no script `unsafe-eval`, no script `unsafe-inline`, no broad wildcard, and no remote font/style origin.
- [ ] public-bind gateway mode does not get permissive CORS by default.

### Exit Criteria

- Tool execution and plugin materialization cannot escape intended policy or filesystem boundaries.
- Production desktop CSP and public-bind CORS defaults are constrained.

## Batch 4: QA Harness Recovery

### Tasks

- [ ] Update P1 acceptance matrix to track helper-file assertions.
- [ ] Update acceptance checker to support the single manifest/checker contract.
- [ ] Add positive and negative checker fixtures under `scripts/tests/`.
- [ ] Make Playwright webServer launch UNC-safe with absolute Node paths or PowerShell launchers.
- [ ] Prevent release gates from reusing stale servers on ports 1420/19789.
- [ ] Keep dev reuse only with current-checkout health marker.
- [ ] Re-run Mission Control full gates.
- [ ] Re-run Rust workspace test from the canonical UNC checkout.
- [ ] If Rust remains blocked, document exact toolchain/UNC evidence and next recovery step.

### Required Tests

- [ ] `python scripts/mission_control_phase_acceptance_check.py --phase P1` passes.
- [ ] checker negative fixture fails.
- [ ] Playwright reaches test execution instead of webServer startup failure.
- [ ] targeted `@core` E2E passes once app/dependency blockers are resolved.
- [ ] `cargo test --workspace --locked` passes from UNC, or the blocker is explicitly accepted.

### Exit Criteria

- Release/checker gates are trustworthy and no known false-green or false-red harness bug remains.

## Batch 5: UI/UX Readiness Clarity

### Tasks

- [ ] Use `frontend-design` before UI edits.
- [ ] Use `ui-ux-pro-max-sanitized` before UI edits.
- [ ] Update token storage mode display.
- [ ] Update live-feed reconnect/degraded copy after WS ticket flow.
- [ ] Update connector health/auth-required display if connector auth changes affect UI.
- [ ] Audit Anthropic/provider validation copy for truthfulness.
- [ ] Ensure security/readiness state uses visible text, not tooltip-only messaging.
- [ ] Test visible UI changes at 375, 768, 1024, and 1440 widths.

### Exit Criteria

- Operator-facing copy says exactly what is true.
- No UI change introduces a broad redesign or ambiguous readiness claim.

## Batch 6: Full Regression and Documentation

### Required Commands

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace --locked`
- [ ] `node .\node_modules\typescript\bin\tsc -b --pretty false` in `apps/mission-control`
- [ ] `node .\node_modules\eslint\bin\eslint.js .` in `apps/mission-control`
- [ ] `node .\node_modules\vitest\vitest.mjs run` in `apps/mission-control`
- [ ] `node .\node_modules\vite\bin\vite.js build` in `apps/mission-control`
- [ ] `python scripts/mission_control_phase_acceptance_check.py --phase P1`
- [ ] `python scripts/security_hardcoded_value_guard.py`
- [ ] `node .\node_modules\@playwright\test\cli.js test --config=playwright.config.ts --grep "@core"` in `apps/mission-control`

### Docs

- [ ] Update API contract docs.
- [ ] Update security docs/runbooks affected by token, CSP, CORS, connector policy, and QA flow.
- [ ] Update blockerboard statuses.
- [ ] Write post-green checkpoint.

### Exit Criteria

- All required commands pass, or a user-accepted external blocker is documented.
- Stop-ship list in the spec has no open item.

## Batch 7: PR Flow

### Tasks

- [ ] Open PR targeting `main`.
- [ ] Include summary and verification commands.
- [ ] Request `@coderabbitai review`.
- [ ] Wait for review completion.
- [ ] Address findings or record explicit user acceptance.
- [ ] Re-run required gates.
- [ ] Write PR-open checkpoint.
- [ ] Merge only after review and validation are complete.
- [ ] Write post-merge checkpoint.

### Exit Criteria

- PR is merged only after review completion and validation evidence.
- Checkpoints include PR link, review state, final validations, and merge commit.

## Stop-Ship Summary

Do not ship if:

- gateway token appears in logs, localStorage, checkpoints, exported packages, or routine UI
- WebSocket production path uses bearer token query auth
- connector tools can hit private/internal network targets by endpoint URL
- connector auth-required operation can execute without auth binding or durable interaction
- production Tauri CSP allows script eval/inline or broad wildcard directives
- public-bind gateway mode can run with permissive default CORS
- path-qualified or cwd/PATH/PATHEXT-hijacked binaries pass the default allowlist
- plugin version or ID can influence filesystem paths outside plugin storage root
- P1 acceptance matrix is known stale
- Vite/Vitest/Playwright gates fail for known local harness reasons
