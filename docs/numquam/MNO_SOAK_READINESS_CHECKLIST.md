# Numquam Soak Readiness Checklist

## Purpose
Multi-day readiness checklist for Numquam integration stability, fallback correctness, and writeback safety before promotion.

## Soak Entry Criteria
1. `cargo check -p carsinos-gateway --bin carsinos-gateway` passes.
2. `cargo test -p carsinos-gateway numquam_ -- --nocapture` passes.
3. `cargo test -p carsinos-gateway --test e2e_process` passes.
4. `memory.preflight` scheduler job succeeds with `health_status=ok`.

## Daily Soak Checks
1. Fallback rate:
- Confirm runs continue when Numquam is degraded/down.
- Track frequency of `numquam.fallback` audit events.
2. Breaker behavior:
- Verify breaker opens after repeated Numquam failure and auto-recovers on successful handshake.
3. Context budget:
- Verify `memory.context_truncated` metadata appears deterministically for oversized context.
4. Writeback stability:
- Verify `writeback.propose` idempotency and approval-driven resolve behavior.
5. Transport parity (if dual):
- Run `memory.parity_probe` and confirm `parity_match=true`.

## Pass/Fail Targets
1. No run-blocking dependency failures caused by Numquam outages.
2. No unresolved critical contract mismatch in capabilities/required operations.
3. No writeback duplication from replay/idempotency-key paths.
4. Breaker and degrade states visible in `/api/v1/status` and `/api/v1/jobs/status`.

## Rollback Plan
1. Set runtime config `memory.blend_mode=local_fallback_only`.
2. Disable Numquam by runtime config `memory.numquam.enabled=false` if required.
3. Continue local-memory sync via `memory.sync` jobs.
4. Keep approvals operational and hold Numquam writeback until preflight recovers.
5. Re-enable in stages:
- `memory.preflight` green
- optional `memory.parity_probe` green
- set blend mode back to `mno_primary` or `local_augment`
