# AppDex Scratch Pad - Core Loop Adds (CEO/Operator Version)

## Goal
Keep CarsinOS focused on OpenClaw-like core behavior (agentic loop + autonomy), not feature-for-feature cloning.

## What To Add Next (Pragmatic Priority)
1. Always-on channel listeners for Telegram and Discord.
- Today, inbound is mostly endpoint-driven.
- Add native long-running listeners so CarsinOS is always "awake" for new messages.

2. Scheduler depth upgrade.
- Keep current working scheduler, but add richer scheduling modes (`at`, `every`, `cron`) and clearer routine controls.
- This improves true autonomous operations.

3. Production trust contract finalization.
- Lock final internet-facing auth/trust values (JWT issuer/audience/proxy/TLS model) so deployment is safe and deterministic.

4. Complete real channel action depth.
- `send`/`reply` are the strongest paths now.
- Finish true transport behavior for other actions (`pin`, `reaction`) where needed.

5. Extension runtime hardening (phase 2).
- Move from "registry + hooks + skills" foundation to full plugin execution lifecycle (install/update/rollback/safety controls).

## Current Agent Tools Available Right Now
1. `tool.exec <command>`
- Runs allowlisted shell commands only.
- Approval: required.

2. `tool.fs_read <path>`
- Reads files inside allowed roots.
- Approval: not required.

3. `tool.fs_write <path>|<content>`
- Writes/appends files inside allowed roots.
- Approval: required.

4. `tool.process <list|status|terminate> [pid]`
- Process operations (`terminate` is high risk).
- Approval: `terminate` requires approval; `list`/`status` do not.

5. `tool.web_search <query>`
- Web search via configured endpoint.
- Approval: not required.

6. `tool.web_fetch <url>`
- Fetches HTTP/HTTPS URLs.
- Approval: not required.

7. Channel actions
- `tool.channel_send <provider:target>|<text>`
- `tool.channel_reply <provider:target>|<text>`
- `tool.channel_pin <provider:target>`
- `tool.channel_reaction <provider:target>|<emoji>`
- Approval: required.
- Current practical scope: Telegram + Discord are first-class.

## Guardrails Already In Place
- Run time limits, tool call caps, provider attempt caps.
- Token/cost budget enforcement + kill-switch behavior.
- Per-session run-lane locking and scheduler single-instance lock.
- Circuit breakers and stop-reason observability.
