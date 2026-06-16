# ExecAss Wakeup Coverage/Audit Task List

> Track: `EXECASS_WAKEUP_COVERAGE_AUDIT WORK`
> Goal: make `execass.wakeup` auditable inside CarsinOS and prove each meaningful CarsinOS change shape either stays quiet or wakes ExecAss with a clear reason.

## Current Baseline

- `execass.wakeup` is implemented in `crates/carsinos-gateway/src/main.rs`.
- Quiet wakeups already avoid LLM calls when no attention item is found.
- Escalated wakeups already call `session.run` when the preflight finds requested approvals or failed scheduled jobs.
- Job run audit data already persists in `job_runs.output_json` and is exposed through `GET /api/v1/jobs/{job_id}/history`.
- Mission Control Calendar can fetch recent job history and render `execass.wakeup` receipts without requiring raw SQLite access.

## Done Means

- Every `execass.wakeup` output includes a stable `coverage_version`.
- Every wakeup output includes `checked` categories, even when it stays quiet.
- Every checked category reports a status: `clear`, `attention`, `unavailable`, or `error`.
- A no-change heartbeat remains quiet and records `llm_invoked: false`.
- A meaningful change in each covered category produces at least one `attention_items[]` entry.
- Escalation output preserves the local preflight evidence and the downstream `session.run` evidence.
- Mission Control or the existing job history API can display/audit the heartbeat packet without reading raw SQLite.

## Coverage Categories

- `jobs`: failed runs, job `last_error`, disabled-but-due jobs, stale leases, jobs due but scheduler disabled.
- `approvals`: pending operator approvals, especially tool, memory.writeback, and bridge execution approvals.
- `tasks`: blocked tasks, overdue tasks, high-priority unowned tasks, linked job failures.
- `boards`: cards in blocked/stale columns, cards linked to failed jobs or overdue tasks.
- `memory`: pending memory writeback proposals, failed memory sync/preflight jobs, stale memory source status.
- `runbooks`: recent failed runbook-backed runs, missing runbook detail for active work, stale active runbooks.
- `channels`: inbound agent mail, Discord, Telegram, or other service messages waiting for assistant attention.
- `connectors`: unhealthy connectors, paused connector interactions, failed connector conversions.
- `providers`: open provider/job/Numquam circuit breakers, auth profile disabled or usage budget issues.
- `agent_mail`: unread or unacked agent mail threads involving the assistant.

## Task 1: Stable Audit Packet

Files:
- Modify: `crates/carsinos-gateway/src/main.rs`
- Test: `crates/carsinos-gateway/src/main.rs`

- [x] Add constants:
  - `EXECASS_WAKEUP_COVERAGE_VERSION`
  - category names for `jobs`, `approvals`, `tasks`, `boards`, `memory`, `runbooks`, `channels`, `connectors`, `providers`, `agent_mail`
- [x] Add helper output shape:
  - `checked[]` entries include `category`, `status`, `summary`, `count`, and optional `error`.
  - `attention_items[]` entries include `kind`, `category`, `summary`, and stable IDs where available.
- [x] Add failing test: quiet wakeup output contains `coverage_version`, all initial checked categories, zero attention items, and `llm_invoked: false`.
- [x] Implement the minimal audit packet so the quiet test passes.
- [x] Add assertion that escalated wakeups also include `coverage_version`, `checked`, `attention_items`, and `session_run`.

## Task 2: Jobs And Approvals Detector Hardening

Files:
- Modify: `crates/carsinos-gateway/src/main.rs`
- Test: `crates/carsinos-gateway/src/main.rs`

- [x] Move current job/approval detection into focused helper functions.
- [x] Add tests for pending approval escalation with category `approvals`.
- [x] Add tests for failed job-run escalation with category `jobs`.
- [x] Add tests that the wakeup job itself is excluded from job-watch false positives.
- [x] Keep result caps deterministic so a noisy system cannot create giant prompt payloads.

## Task 3: Tasks And Boards Detector

Files:
- Modify: `crates/carsinos-gateway/src/main.rs`
- Test: `crates/carsinos-gateway/src/main.rs`

- [x] Detect blocked tasks via `blocked_reason`.
- [x] Detect overdue tasks via `due_at < now`.
- [x] Detect high-priority unowned active tasks.
- [x] Detect board cards linked to failed jobs.
- [x] Detect board cards linked to overdue tasks.
- [x] Add one test per detector and prove each escalates with category `tasks` or `boards`.

## Task 4: Memory And Runbooks Detector

Files:
- Modify: `crates/carsinos-gateway/src/main.rs`
- Test: `crates/carsinos-gateway/src/main.rs`

- [x] Detect pending `memory.writeback` approvals.
- [x] Detect failed memory scheduled jobs by mode prefix `memory.`.
- [x] Detect stale or failing memory preflight state where an existing storage surface supports it.
- [x] Detect recent failed assistant-session runbooks or active runbooks attached to failed runs.
- [x] Detect failed scheduled-job runbook attention for failed job runs.
- [x] Add tests proving supported memory/runbook attention items escalate and include enough IDs for audit.

## Task 5: Channels, Connectors, Providers, Agent Mail

Files:
- Modify: `crates/carsinos-gateway/src/main.rs`
- Test: `crates/carsinos-gateway/src/main.rs`

- [x] Detect unread/unacked agent mail threads for the assistant.
- [x] Detect paused/waiting connector interactions.
- [x] Detect failed connector conversions.
- [x] Detect disabled connector assignments.
- [x] Detect unhealthy connector sources where storage exposes that state.
- [x] Detect open provider circuit breakers.
- [x] Detect open job and Numquam circuit breakers.
- [x] Detect auth profile disabled states that affect the configured assistant agent.
- [x] Add one focused test per supported storage shape implemented so far.

## Task 6: Mission Control Audit Display

Files:
- Inspect first: `apps/mission-control/src/features/calendar/CalendarPage.tsx`
- Modify if needed: `apps/mission-control/src/lib/api.ts`
- Modify if needed: `apps/mission-control/src/types.ts`
- Test if needed: existing calendar/unit test surface or a new focused test

- [x] Confirm Calendar can fetch job history or add a minimal history fetch control if it cannot.
- [x] Render quiet wakeup audit fields in plain English:
  - status
  - LLM invoked yes/no
  - checked categories
  - attention items
- [x] Keep raw JSON accessible enough for debugging without making normal users read it first.
- [x] Add a focused test or API-type test for parsing `execass.wakeup` output.

## Task 7: Setup-Flow Service Coverage Proof

Files:
- Test/harness location to choose after Task 6.
- Likely touchpoints: Mission Control connector/onboarding tests and gateway job tests.

- [x] Run normal onboarding flow and confirm default ExecAss agent can be configured.
- [x] Create ExecAss Calendar wakeup job and run it.
- [x] Seed or perform service-facing changes for agent mail, connector, and channel-like inputs.
- [x] Confirm the wakeup audit packet records the checked service category.
- [x] Confirm attention-worthy service input escalates to `session.run`.
- [x] Record the exact manual/live flow results in the checkpoint.

Task 7 proof note:
- Isolated runtime proof report: `reports/execass-wakeup-task7/task7-proof-20260603-031715.json` in the local validation artifact bundle.
- Quiet run `d2627097-5cdc-460d-9c85-6c10c92b555a` stayed `status: quiet`, `llm_invoked: false`, and checked all ten coverage categories.
- Agent-mail service seed `8a33c742-5233-41de-b73c-bb72e00cd0e9` woke job `ceaf8c5c-cc59-469f-8ff3-40708304451d`.
- Attention run `50e9dc43-c33f-438b-a8ef-a452c008a4ff` escalated with category `agent_mail`, `llm_invoked: true`, and downstream `session_run.run_status: succeeded`.
- Mission Control setup-flow proof passed via production build + Vite preview + mock gateway: `npm exec -- playwright test --config=playwright.config.ts e2e/connectors.spec.ts` with `MC_E2E_BASE_URL=http://127.0.0.1:1420`, 2 passed. This covered normal quickstart onboarding, Discord channel-like quick setup, and connector import/convert/publish/assign/auth setup without real third-party credentials.
- Mission Control web server served from Windows at `http://127.0.0.1:1420`, but the in-app browser bridge could not complete UI smoke because Vite rejected `host.docker.internal` under `server.allowedHosts`; normal browser access from Windows should use `127.0.0.1`.

## Verification Gates

- [x] `cargo test -p carsinos-gateway execass_wakeup --locked -- --nocapture`
- [x] `cargo fmt --all -- --check`
- [x] `npm run typecheck` from a mapped drive if Mission Control files change
- [x] Focused Mission Control unit tests if Calendar/API UI files change
- [x] `git diff --check` scoped to touched files and checkpoint files
- [x] Manual/live setup-flow evidence once the detector matrix is green

## Notes

- `heartbeat.run` remains the low-level health heartbeat and should not become the ExecAss orchestration layer.
- `execass.wakeup` owns the richer assistant preflight/audit behavior.
- Do not call cloud/local LLM providers for quiet no-change wakeups.
- Do not claim full coverage until every category above has direct evidence or a documented unsupported storage gap.
- Memory stale-state coverage is limited to persisted failures that CarsinOS can audit locally today: pending `memory.writeback` approvals and failed scheduled jobs whose payload mode starts with `memory.`, including `memory.preflight`. The current storage layer does not persist an independent stale memory-source health record for `execass.wakeup` to scan synchronously.
- Runbook stale-state coverage is limited to persisted run/job evidence CarsinOS can audit locally today: failed scheduled-job runbooks and failed assistant-session runbooks. Missing/stale runbook-detail freshness is generated in the Mission Control read model rather than stored as an independent wakeup source.
