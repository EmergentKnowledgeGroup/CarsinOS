# Autonomy Guardrails Observability (AG-010)

This document defines the operator-visible observability contract added for the autonomy guardrails track.

## Endpoints

- `GET /api/v1/status`
- `GET /api/v1/jobs/status`

## Exposed Fields

Both endpoints now expose:

- `scheduler_lock`: scheduler lock ownership and lock-file metadata.
- `open_circuit_breakers`: count of breaker records currently in `open` state.
- `circuit_breakers[]`: breaker records (`scope`, `target_id`, `state`, `consecutive_failures`, `cooldown_until`, `last_error_code`, `updated_at`).
- `top_stop_reasons[]`: aggregated reason codes from recent job terminal errors.

`GET /api/v1/status` additionally exposes:

- `autonomy_guardrails`: active runtime guardrail config payload (`max_run_ms`, tool/provider budgets, heartbeat timeout, breaker threshold).

## Reason-Code Visibility

The following reason-code families are expected in run/job payloads and may surface through `top_stop_reasons`:

- Budget stops (`BUDGET_*`)
- Breaker stops (`BREAKER_*`)
- Heartbeat guardrail stops (`HEARTBEAT_*`)
- Timeout stops (`TIMEOUT`)

## Operator Usage

Use `/api/v1/status` for control-plane visibility (guardrail config + lock + breaker states), and `/api/v1/jobs/status` for scheduler operational health (jobs due/enabled + breaker/stop summaries).
