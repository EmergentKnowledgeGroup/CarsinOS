# AppDex Ticket Sequence: Safe Autonomy Guardrails (CarsinOS)

## Objective
Implement OpenClaw-style autonomy (heartbeat + cron + automated runs) without runaway token/cost behavior.

## Non-Negotiable Rules
- No prompt-only safety. All critical guardrails must be runtime-enforced.
- Fail closed on budget overflow, unknown states, or lock conflicts.
- One active run per session key.
- One scheduler worker per state directory.
- Every ticket must ship with tests before moving to the next ticket.

## Definition Of Green (for every ticket)
- `cargo test -p carsinos-gateway`
- `cargo test -p carsinos-tools`
- `cargo test -p carsinos-storage`

## AG-001: Guardrail Config Contract (P0)
- Goal: Add first-class runtime config for autonomy guardrails.
- Files: `crates/carsinos-protocol/src/lib.rs`, `crates/carsinos-gateway/src/main.rs`
- Tasks: Add `RuntimeAutonomyGuardrailsConfig` with defaults for max run ms, max tool calls per run, max provider input chars, max tool output chars total, max provider attempts, max consecutive failures before breaker, and heartbeat max run ms. Wire through get/update runtime config APIs.
- Acceptance: Config round-trips via API, defaults are applied when unset, invalid values rejected with clear 400 errors.
- Validation: Add API tests for valid update, invalid bounds, and default fallback.

## AG-002: Per-Session Run Lane Lock (P0)
- Goal: Enforce one active run at a time per session.
- Files: `crates/carsinos-gateway/src/main.rs`
- Tasks: Add session lane lock manager in app state. Acquire lock before `execute_run_with_status_handling` for create run, resume run, channel auto-run, and scheduler session-run. Return 409 for conflicting interactive requests.
- Acceptance: Parallel run requests against same session do not execute concurrently.
- Validation: Add concurrency test that fires simultaneous run requests and confirms serialization.

## AG-003: Run Budget Governor (P0)
- Goal: Hard-stop runaway runs.
- Files: `crates/carsinos-gateway/src/main.rs`
- Tasks: In `execute_run`, enforce guardrails from AG-001: max wall time, max tool calls, max provider input chars, max total tool output chars, max provider attempts. Emit terminal reason codes like `BUDGET_MAX_RUN_MS`, `BUDGET_MAX_TOOL_CALLS`.
- Acceptance: Runs terminate predictably before blowup and store explicit failure reason.
- Validation: Add tests for each budget threshold breach path.

## AG-004: Scheduler Timeout Enforcement (P0)
- Goal: Make job `timeout_ms` real.
- Files: `crates/carsinos-gateway/src/main.rs`
- Tasks: Wrap `execute_job_payload` in `tokio::time::timeout(Duration::from_millis(job.timeout_ms))` inside `execute_job_once`. On timeout, mark job run failed with timeout code and continue scheduler loop safely.
- Acceptance: A slow job times out at configured limit and releases lease.
- Validation: Add scheduler test with intentionally delayed payload and assert timeout failure.

## AG-005: Single Scheduler Lease / Instance Guard (P0)
- Goal: Prevent duplicate scheduler workers on same state dir.
- Files: `crates/carsinos-gateway/src/main.rs`
- Tasks: Add startup lock file or DB lease guard. If lock exists, scheduler loop does not start and a clear error is emitted. Keep gateway API alive, scheduler disabled.
- Acceptance: Starting second gateway does not create a second scheduler executor.
- Validation: Add integration test launching two processes against same state dir.

## AG-006: Tool Fanout Cap + Error Loop Breaker (P1)
- Goal: Prevent tool storm patterns.
- Files: `crates/carsinos-gateway/src/main.rs`
- Tasks: Cap parsed `tool.*` invocations per run. Add repeated error fingerprint guard: if same tool+error repeats N times in one run, abort run with breaker reason.
- Acceptance: Large tool blocks and repeated-error loops terminate early.
- Validation: Add tests for over-cap input and repeated identical failure.

## AG-007: Token/Cost Accounting + Budget Kill Switch (P1)
- Goal: Stop spending when budget is exhausted.
- Files: `crates/carsinos-providers/src/lib.rs`, `crates/carsinos-storage/src/lib.rs`, `crates/carsinos-gateway/src/main.rs`, `migrations/*`
- Tasks: Extend provider response to include usage metrics. Persist run token/cost fields. Add per-auth-profile daily token and USD budget checks before provider call. On breach, deny run and flip provider/profile kill switch state.
- Acceptance: Budget breach blocks further automated spending until manual override.
- Validation: Add deterministic test with mocked usage that trips both token and USD budgets.

## AG-008: Heartbeat Mode (Tools Disabled By Runtime) (P1)
- Goal: Safe heartbeat automation.
- Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`
- Tasks: Add scheduler payload mode `heartbeat.run` with strict runtime policy: tools disabled, short run timeout, no followup retries, output contract `HEARTBEAT_OK` or `ALERT: ...`. Reject tool lines in heartbeat input.
- Acceptance: Heartbeat cannot execute tools under any prompt content.
- Validation: Add tests proving heartbeat ignores or rejects tool requests and enforces output contract.

## AG-009: Circuit Breakers (Provider + Job) (P1)
- Goal: Auto-stop bad loops quickly.
- Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-storage/src/lib.rs`, `migrations/*`
- Tasks: Track consecutive failures for provider profile and for job id. Open breaker at configured threshold. While open, skip execution and emit structured event. Manual or timed reset required.
- Acceptance: Repeated failures stop causing repeated spend.
- Validation: Add tests for breaker open, skip behavior, and reset path.

## AG-010: Observability + Ops UX (P1)
- Goal: Make budget and breaker state visible and actionable.
- Files: `crates/carsinos-gateway/src/main.rs`, `crates/carsinos-protocol/src/lib.rs`, docs under `docs/`
- Tasks: Add status payload fields for active budgets, breaker states, scheduler lock owner, and top failure reasons. Add run/job events with guardrail reason codes.
- Acceptance: Operator can answer "why did this stop" from status/events without digging through raw logs.
- Validation: Add endpoint tests for new status fields and event payload schema tests.

## AG-011: Regression Suite For Runaway Prevention (P0-P1 Gate)
- Goal: Ensure this class of bug does not return.
- Files: `crates/carsinos-gateway/tests/e2e_process.rs`, `crates/carsinos-gateway/tests/common/mod.rs`
- Tasks: Add end-to-end tests for: tool storm cap, repeated error breaker, scheduler timeout, per-session serialization, duplicate scheduler prevention, heartbeat no-tools contract, and budget kill switch.
- Acceptance: Full suite fails if any runaway guardrail regresses.
- Validation: `cargo test -p carsinos-gateway --test e2e_process`

## Execution Order (Strict)
1. AG-001
2. AG-002
3. AG-003
4. AG-004
5. AG-005
6. AG-011 (initial P0 subset)
7. AG-006
8. AG-007
9. AG-008
10. AG-009
11. AG-010
12. AG-011 (full suite)

## Ship Gate
Do not enable heartbeat or autonomous scheduler jobs in production until AG-001 through AG-005 and AG-011 (P0 subset) are green.
