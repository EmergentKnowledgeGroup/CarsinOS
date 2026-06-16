# Agent Mail Auto-Route SpecSwarm Report

Status: complete
Mode: manual fallback
Reason: `codex exec` rejected the UNC checkout path with Windows `os error 161`; the parent agent completed all SpecSwarm passes directly.

## Executive Summary

- The spec is viable if it reuses the existing Agent Mail, session, run, audit, app-kv, and channel ingest paths.
- The highest-risk gaps were durable idempotency, runaway auto-reply loops, synchronous HTTP send latency, and routed-agent runs using channel default models.
- The hardened spec now requires background delivery, durable idempotency, bounded context/fanout, sticky-route expiry, and selected-agent model defaults.
- API-backed teammates are supported only when represented through a run-capable provider/adapter; the implementation must not invent a parallel execution path.
- No blocker remains before implementation.

## Gap And Edge-Case Findings

| Severity | Finding | Folded Fix |
| --- | --- | --- |
| P0 | Duplicate Agent Mail delivery could create duplicate runs/replies after process restart. | Added durable idempotency key requirement backed by `app_kv` or equivalent existing state. |
| P0 | Auto-generated mail replies could recurse forever. | Added `auto_execution=true` skip rule plus fanout/context caps. |
| P0 | HTTP/MCP message send could block while an LLM/API teammate runs. | Added background delivery requirement and testable synchronous internal helper. |
| P0 | Explicit `@agent` routing could select the right session but still use channel default model/provider. | Added selected-agent model/provider default requirement. |
| P1 | Sticky routes could silently mutate long-term routing assignments. | Required TTL-bound app-kv state and no mutation to `routing.assistant_assignments`. |
| P1 | Unknown `@token` could become a message-loss path. | Required fallback to existing route/default agent with audit evidence. |
| P1 | API-call teammates are ambiguous in the current code. | Defined v1 scope as run-capable agents; API-backed teammates need provider/connector adapter representation. |

## Implementation Touchpoint Map

| Spec Section | Likely Touchpoints | Tests/Gates | Risks |
| --- | --- | --- | --- |
| AMR-1..AMR-5 | `send_agent_mail_message`, `execute_agent_mail_mcp_tool`, storage mail APIs, `execute_run_with_lane_control`, `latest_assistant_reply_text` | gateway `agent_mail` tests; storage `agent_mail` tests | duplicate replies, run lane conflicts, hidden synchronous latency |
| AMR-6 | local Codex skill folder or repo skill template docs | smoke with gateway/MCP when runtime available | credential/runtime availability |
| CHR-1..CHR-4 | `ingest_telegram_channel_message`, `ingest_discord_channel_message`, channel adapter parsing helpers, runtime config validation, `app_kv` | gateway channel tests; channel crate parser tests | unknown mention parsing, wrong model defaults, cross-user lane leakage |
| Runtime Config | protocol runtime config structs, config validation, update/get runtime config tests | protocol/gateway config tests | overly broad config churn |
| Audit/Events | `emit_event`, `record_security_audit`, ExecAss wakeup scanner | focused wakeup/agent_mail tests | false green if only events are asserted |

## Over-Engineering And Reward-Hacking Review

- Avoid a new queue table unless app-kv idempotency cannot safely encode status.
- Avoid fuzzy agent-name routing until exact ids prove the workflow.
- Do not write tests that only check that an event emitted; tests must verify actual run/reply/session behavior.
- Do not mark API-backed teammate support complete unless it executes through a run-capable adapter or records a deterministic not-runnable outcome.
- Do not loosen channel allowlist or mention gating to make explicit routing tests pass.

## Final QA

No contradictions remain after fold-in. The implementation can proceed in three slices:

1. Agent Mail auto-execution backend.
2. Explicit and sticky channel routing backend.
3. Codex skill/docs plus final validation/staging.

The checklist and blockerboard are locked alongside the spec.
