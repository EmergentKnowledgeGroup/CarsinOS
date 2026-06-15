# Goal Prompt: Agent Mail Auto-Execution And Explicit Channel Routing

You are Codex executing the `AGENT_MAIL_AUTOROUTE WORK` track in the CarsinOS repo.

## Objective

Implement the locked spec in `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_SPEC.md` completely enough for CodeRabbit review:

- Agent Mail addressed directly to a run-capable agent auto-executes that agent and posts the reply back to the same thread.
- ExecAss remains in the observer/escalation loop but does not have to perform routine mail dispatch.
- Discord and Telegram support exact first-token `@agent_id` routing, unknown-token fallback, selected-agent model/provider defaults, and bounded sticky follow-up routing.
- Add or document a Codex-app skill workflow for sending Agent Mail to a specific agent and waiting for a reply.

## Required Sources

- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_SPEC.md`
- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_EXECUTION_CHECKLIST.md`
- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_BLOCKERBOARD.md`
- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_SPECSWARM_REPORT.md`
- repo checkpoint SOP in `AGENTS.md`

## Execution Rules

- Follow the checklist IDs and update `CHECKPOINT.md` after every code/doc change batch.
- Update `runtime/checkpoints/LATEST.md` and `LATEST.json` at post-green tests and PR open.
- Do not create worktrees; use this local checkout as source of truth.
- Preserve unrelated user/localFS work.
- Use existing CarsinOS storage/session/run/audit/config patterns; do not add a second execution system.
- Do not fake API-backed teammate support. Unsupported/non-runnable recipients must produce deterministic evidence.
- Do not loosen channel authorization or mention gating.
- Do not mark checklist items complete without test evidence.

## Verification Gates

At minimum:

```powershell
cargo fmt --all -- --check
cargo test -p carsinos-gateway agent_mail --locked -- --nocapture
cargo test -p carsinos-gateway channel --locked -- --nocapture
cargo test -p carsinos-storage agent_mail --locked -- --nocapture
```

Broaden to protocol/workspace tests if runtime config or shared contracts change.

## Done

- All checklist items are complete or explicitly blocked with evidence.
- Focused tests prove real behavior, not just emitted events.
- Checkpoints are current.
- Related files are staged, committed, pushed, and a draft PR to `main` is opened for CR review.
