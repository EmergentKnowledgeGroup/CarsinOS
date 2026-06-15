# Agent Mail Auto-Route Execution Checklist

Source spec: `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_SPEC.md`
Owner track: `AGENT_MAIL_AUTOROUTE WORK`

## Phase 0: Baseline And Red Tests

- [x] `AMR-0.1` Confirm checkpoint state, branch, and dirty tree before implementation edits.
- [x] `AMR-0.2` Add focused failing tests for direct Agent Mail recipient auto-execution.
- [x] `AMR-0.3` Add focused failing tests for mail idempotency and anti-loop behavior.
- [x] `CHR-0.1` Add focused failing tests for Discord `@agent` routing and fallback.
- [x] `CHR-0.2` Add focused failing tests for Telegram `@agent` routing and sticky follow-up.

## Phase 1: Agent Mail Auto-Execution

- [x] `AMR-1.1` Add internal recipient-resolution helper using exact active `agent_id` match.
- [x] `AMR-1.2` Add durable idempotency status using existing runtime state, preferably `app_kv`.
- [x] `AMR-1.3` Add testable internal mail auto-execution function that creates/reuses the mail session, writes bounded context, runs the selected agent, posts the reply, and acks the triggering recipient.
- [x] `AMR-1.4` Wire HTTP and MCP mail send paths to schedule background delivery without blocking normal send responses.
- [x] `AMR-1.5` Emit audit/websocket events for scheduled, skipped, busy, succeeded, and failed outcomes.
- [x] `AMR-1.6` Keep ExecAss wakeup visibility for unresolved/unacked failures.

## Phase 2: Explicit And Sticky Channel Routing

- [x] `CHR-1.1` Add exact first-token `@agent_id` parser with punctuation-safe stripping.
- [x] `CHR-1.2` Add runtime sticky-route state keyed by platform, conversation, human/platform user, and agent id with expiry.
- [x] `CHR-1.3` Route Discord explicit mentions to selected agent session and selected agent model/provider defaults.
- [x] `CHR-1.4` Route Telegram explicit mentions to selected agent session and selected agent model/provider defaults.
- [x] `CHR-1.5` Unknown `@token` falls back to current route/default agent and is auditable.
- [x] `CHR-1.6` Sticky route follow-up works without mutating long-term assistant assignments.

## Phase 3: Codex Skill / Operator Workflow

- [x] `SKILL-1.1` Add a local Codex skill or repo-documented skill template for sending Agent Mail to a specific agent and waiting for a reply.
- [x] `SKILL-1.2` The skill uses gateway HTTP/MCP surfaces only and does not touch SQLite directly.
- [x] `SKILL-1.3` The skill documents required gateway URL/token environment variables and timeout behavior.

## Phase 4: Validation And PR Packaging

- [x] `VAL-1.1` Run `cargo fmt --all -- --check`.
- [x] `VAL-1.2` Run `cargo test -p carsinos-gateway agent_mail --locked -- --nocapture`.
- [x] `VAL-1.3` Run `cargo test -p carsinos-gateway channel --locked -- --nocapture`.
- [x] `VAL-1.4` Run `cargo test -p carsinos-storage agent_mail --locked -- --nocapture`.
- [x] `VAL-1.5` Run broader workspace/protocol tests if runtime config or protocol contracts change. No protocol/runtime config structs changed in this slice; constants are recorded as the safe v1 implementation knob surface.
- [x] `VAL-1.6` Update checkpoints post-green.
- [ ] `VAL-1.7` Stage only directly related files, commit, push, and open a draft PR to `main` for CodeRabbit review.

## Recorded Follow-Up

- [ ] `CFG-FU-1` Promote Agent Mail auto-execution and explicit channel routing limits from v1 constants into the runtime config UI/API once the first backend behavior lands cleanly.
