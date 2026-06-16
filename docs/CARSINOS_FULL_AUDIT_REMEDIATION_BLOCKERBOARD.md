# carsinOS Full Audit Remediation Blockerboard

Date: 2026-05-04
Source spec: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_SPEC.md`
Execution checklist: `docs/CARSINOS_FULL_AUDIT_REMEDIATION_EXECUTION_CHECKLIST.md`
Current status: implemented and validated on 2026-05-04

## Purpose

Execution control board for the full audit remediation work.

Use this board to track blockers, owners, validation gates, release eligibility, and deferrals. A remediation item may not be treated as release-ready unless its blocker row is `DONE` or its deferral fields explicitly allow a release PR.

## Final Validation Evidence

- `cargo fmt --all -- --check`: PASS
- `cargo test --workspace --locked -j 1`: PASS
- Mission Control PR quality gate: PASS (`runtime/quality-gate/reports/quality-gate-pr-20260504T132422Z.json`)
- Mission Control release quality gate: PASS (`runtime/quality-gate/reports/quality-gate-release-20260504T152129Z.json`)
- Mission Control full Playwright E2E: PASS, 35/35
- Mission Control core Playwright E2E: PASS, 25/25 inside the PR quality gate
- Mission Control Tauri smoke: PASS, Z-drive artifacts under `Z:\carsinos-codex-work\carsinos\tauri-smoke-artifacts`
- Mission Control Tauri debug no-bundle build: PASS, binary under `Z:\carsinos-codex-work\carsinos\cargo-target\debug\carsinos-mission-control.exe`
- Mission Control unit tests: PASS, 24 files / 122 tests
- Mission Control typecheck/lint/build: PASS
- Mission Control P1-P4 acceptance checks: PASS
- Hardcoded-value guard: PASS (`runtime/security/reports/hardcoded-value-guard-20260504T122020Z.json`)

## Current Stop-Ship State

No implementation blocker remains open for the nine audit findings. `B7` PR/review/merge remains a normal repository workflow step, not an implementation blocker.

## Status Legend

- `TODO`: defined, not started
- `IN_PROGRESS`: actively being implemented
- `BLOCKED`: cannot proceed without dependency or decision
- `VERIFYING`: implemented and under validation
- `DONE`: implemented, validated, and checkpointed
- `DEFERRED`: explicitly accepted deferral with release eligibility recorded

## Deferral Fields

Every deferred blocker must record:

- owner
- accepted_by
- reason
- affected_gate
- release_pr_allowed
- expiry_or_revisit_condition
- next_command_or_evidence

If any of those fields are missing, the deferral is invalid and the blocker remains stop-ship.

## Global Blockers Register

| Blocker ID | Owner | Status | Blocker | Impact | Affected Gate | Release PR Allowed | Accepted By | Expiry / Revisit | Unblocks |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BLK-01 | Planning | DONE | Remediation spec/checklist/blockerboard must exist and cross-link before implementation | Implementation starts from an unlocked plan otherwise | docs QA | yes | Codex planning lane | locked docs exist | all batches |
| BLK-02 | QA/Frontend | DONE | Mission Control Rollup optional native package missing from `node_modules` | Vitest and Vite build cannot run | unit/build | yes | Codex implementation lane | `npm ls @rollup/rollup-win32-x64-msvc` passes after mapped-drive `npm install` | B0, B4, B6 |
| BLK-03 | QA/Rust | DONE | Rust workspace tests fail compiling dependencies from UNC/shared-target toolchain state | Resolved by using Z-drive Cargo target/cache and sequential workspace tests | `cargo test --workspace --locked -j 1` | yes | Codex implementation lane | full workspace Rust regression passed from UNC checkout with Z-backed target | B6, PR |
| BLK-04 | Gateway/Auth | DONE | WebSocket bearer query auth and URI trace logging expose tokens | Gateway token can leak into logs | gateway tests/security guard/E2E | yes | Codex implementation lane | one-time WS ticket path plus query redaction tests pass | B1 |
| BLK-05 | Gateway/Auth | DONE | Generated gateway bearer token is logged | Runtime secret must not persist to stdout/file logs | gateway tests/security guard | yes | Codex implementation lane | token field removed from generated-token log and hardcoded guard is green | B1 |
| BLK-06 | Mission Control Runtime | DONE | Browser fallback stores gateway token in localStorage | Violates API contract and expands XSS/session risk | frontend unit/E2E/API contract | yes | Codex implementation lane | legacy key purged; prod browser path is memory-only and E2E uses sessionStorage | B1, B5 |
| BLK-07 | Gateway/Connectors | DONE | Connector execution lacks SSRF/DNS/redirect policy | Connector tools can target internal/private services | gateway connector tests | yes | Codex implementation lane | import/execution policy tests pass; connector client redirects disabled | B2 |
| BLK-08 | Gateway/Connectors | DONE | Connector auth-required state is too coarse/dead | Secured operations can execute without auth repair flow | gateway connector tests/UI health | yes | Codex implementation lane | per-operation auth metadata, health counts, and UI auth coverage are implemented and tested | B2, B5 |
| BLK-09 | Tools Runtime | DONE | Tool binary allowlist allows path/PATHEXT hijack | Arbitrary executable can masquerade as allowlisted binary | `carsinos-tools` tests | yes | Codex implementation lane | path-qualified binaries are rejected and `carsinos-tools --lib` passes | B3 |
| BLK-10 | Plugin Runtime | DONE | Plugin version/path containment can escape storage root | Plugin materialization can write outside intended root | gateway plugin tests | yes | Codex implementation lane | plugin version normalization and contained bundle path tests pass | B3 |
| BLK-11 | Desktop Security | DONE | Production Tauri CSP allows inline/eval and remote fonts | Desktop XSS/offline boundary is weak | static CSP/build/Tauri smoke | yes | Codex implementation lane | CSP removes script inline/eval and remote font/style origins; Mission Control build passes | B3, B5 |
| BLK-12 | Gateway/CORS | DONE | Public-bind mode can inherit permissive CORS defaults | Public gateway can accept broad browser origins | gateway CORS tests | yes | Codex implementation lane | public-bind CORS test passes | B3 |
| BLK-13 | QA/Acceptance | DONE | P1 acceptance matrix is stale and file-string brittle | Release checker false-red or false-green risk | acceptance checker | yes | Codex implementation lane | P1-P4 acceptance checks pass | B4 |
| BLK-14 | QA/E2E | DONE | Playwright webServer is not UNC-safe and can false-green with stale ports | E2E cannot reliably start from local checkout | Playwright `@core` and full E2E | yes | Codex implementation lane | Z-backed Playwright browser install plus polling env gives core 25/25 and full 35/35 pass | B4 |
| BLK-15 | UX/Readiness | DONE | Operator UI copy/storage/readiness states can misrepresent security state | Users may believe unsafe or unready states are ready | frontend UI tests/responsive QA | yes | Codex implementation lane | token storage copy, connector auth coverage, unit tests, and E2E pass | B5 |

## Batch Board

| Batch ID | Batch | Status | Depends On | Blockers | Exit Criteria |
| --- | --- | --- | --- | --- | --- |
| B0 | Planning artifact and minimum gate prep | DONE | BLK-01 | BLK-02, BLK-03 | Minimum gates known green/blocked before security edits |
| B1 | Token and log hardening | DONE | B0 | BLK-04, BLK-05, BLK-06 | No token in production URL/log/localStorage paths |
| B2 | Connector network and auth safety | DONE | B0 | BLK-07, BLK-08 | Connector execution is network/auth fail-closed |
| B3 | Containment and desktop hardening | DONE | B0 | BLK-09, BLK-10, BLK-11, BLK-12 | Tool/plugin/CSP/CORS boundaries are constrained |
| B4 | QA harness recovery | DONE | B0, BLK-02 | BLK-13, BLK-14 | Acceptance and E2E gates are trustworthy from UNC checkout |
| B5 | UI/UX readiness clarity | DONE | B1, B2, B3 | BLK-15 | UI displays exact security/readiness truth |
| B6 | Full regression and documentation | DONE | B1, B2, B3, B4, B5 | BLK-03 | Required verification matrix green |
| B7 | PR flow | TODO | B6 | any open no-release blocker | PR reviewed, revalidated, merged, and checkpointed |

## Critical Path

1. `BLK-02` dependency repair
2. `B0` minimum gates and checkpoint
3. `B1` token/log/storage hardening
4. `B2` connector network/auth hardening
5. `B3` containment/CSP/CORS hardening
6. `B4` acceptance/E2E recovery
7. `B5` UI truthfulness pass
8. `B6` full regression
9. `B7` PR review/merge flow

## Parallel Work Lanes

- `B1`, `B2`, and `B3` may run in separate implementation tracks only if their write sets are coordinated.
- Do not run two workers against `crates/carsinos-gateway/src/main.rs` at the same time.
- `B4` can begin after dependency repair, but release E2E cannot be considered final until token and UI changes are folded in.
- `B5` must wait for the relevant backend/runtime truth to exist so the UI does not encode guesses.

## Known Collision Risks

- `crates/carsinos-gateway/src/main.rs` is already dirty and is the highest-collision file.
- `apps/mission-control/src/lib/api.ts`, `apps/mission-control/src/lib/runtime.ts`, `apps/mission-control/src/lib/ws.ts`, E2E helpers, and Playwright config are shared frontend seams.
- `crates/carsinos-protocol/src/lib.rs` and `crates/carsinos-storage/src/lib.rs` are shared contract surfaces.
- Acceptance matrix/script changes must stay in one checker contract.

## Backout Levers

- Disable connector-derived execution while preserving connector records.
- Revert the WS ticket/client commit as a unit before release.
- Use E2E-only sessionStorage flag for tests without restoring production localStorage.
- Use dev-only CSP or local asset fixes without weakening production CSP.
- Use local-dev-only Playwright server reuse with current-checkout health marker.

## Forbidden Backouts

- Restore production `?token=` bearer query auth.
- Restore token logging.
- Restore production localStorage token persistence.
- Bypass SSRF policy for all connectors.
- Forward connector auth before policy validation.
- Restore production script `unsafe-eval` or script `unsafe-inline`.
- Allow public-bind permissive CORS by default.
- Allow release gates to reuse unknown stale servers.

## Stop-Ship Triggers

- Any no-release blocker remains `TODO`, `IN_PROGRESS`, `BLOCKED`, or invalidly deferred.
- Any required verification gate fails without explicit accepted blocker.
- Any secret/token exposure path remains open.
- Any implementation adds a second runtime, second auth store, second connector policy path, or second acceptance format.
