# Numquam Pipeline Hooks Runbook

## Scope
This runbook defines the operator-facing hook points exposed by carsinOS for future Numquam pipeline orchestration work (import, backfill, eval), without requiring run-loop refactors.

## Hook Surfaces
1. Scheduler job mode `memory.pipeline.hook`
- Required payload key: `hook` (string)
- Optional payload key: `hook_payload` (object)
- Behavior: emits `memory.pipeline.hook` event and stores job output envelope.

2. Scheduler job mode `memory.preflight`
- Validates current Numquam integration handshake status.
- Optional payload key: `fail_on_degrade` (bool) to fail hard when degraded.

3. Scheduler job mode `memory.parity_probe`
- Runs HTTP/MCP parity probe when transport is `dual`.
- Emits `job.memory.parity_probe` event with parity result.

## Recommended Hook Names
1. `memory.import.start`
2. `memory.import.chunk`
3. `memory.import.complete`
4. `memory.backfill.start`
5. `memory.backfill.complete`
6. `memory.eval.start`
7. `memory.eval.complete`

## Operator Run Steps
1. Verify `/api/v1/status` shows Numquam enabled and not breaker-open.
2. Run `memory.preflight` job and confirm `health_status=ok`.
3. For dual transport, run `memory.parity_probe` and verify `parity_match=true`.
4. Execute `memory.pipeline.hook` jobs for your target pipeline phase.
5. Inspect `job.memory.*` and `memory.pipeline.hook` events and `/api/v1/security/audit`.

## Failure Handling
1. If preflight fails with `DEPENDENCY_UNAVAILABLE`, keep run execution on stateless/local fallback and do not run mutation hooks.
2. If parity probe reports mismatch, disable dual parity-sensitive workflows and keep writeback approvals manual-only.
3. If breaker is open, wait for cooldown or restore dependency health and re-run preflight.
