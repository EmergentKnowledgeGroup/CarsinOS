# Agent Mail Auto-Execution And Explicit Channel Routing Spec

Status: locked after SpecSwarm manual fallback
Owner track: `AGENT_MAIL_AUTOROUTE WORK`
Branch: `codex/agent-mail-autoroute-20260615`

## Goal

Make CarsinOS route work to the right teammate without forcing ExecAss to be the only active dispatcher.

This work adds two first-class behaviors:

1. Agent Mail addressed directly to an agent auto-executes that recipient and posts the recipient's reply back to the same thread.
2. Telegram and Discord messages can explicitly route to a teammate with `@agent` syntax, with optional sticky routing for follow-up turns.

ExecAss remains in the loop as an observer/escalation surface, but direct teammate mail and direct channel routing must not require ExecAss to perform the routine dispatch.

## SpecSwarm Lock Notes

SpecSwarm was run in manual fallback mode because `codex exec` rejected the UNC checkout path with Windows `os error 161`. The fallback folded four review passes into this locked spec:

- gap and edge-case review
- implementation touchpoint mapping
- over-engineering and reward-hacking prevention review
- final QA consolidation

The main folded fixes are durable idempotency, background delivery, routed-agent model defaults, strict sticky-route scoping, and a smaller v1 scope that reuses existing sessions/runs/storage instead of adding a second execution system.

## Existing Ground Truth

- Agent records already carry `agent_id`, model provider/model id, role metadata, and optional memory binding.
- Agent Mail already persists threads, participants, messages, recipients, acknowledgements, attachments, HTTP APIs, MCP facade tools, and websocket events.
- `assistant.mail.*` and `agent_mail.*` tools currently store/fetch/ack mail but do not auto-run recipients.
- Channel ingest already resolves linked human identity + assistant assignment into a canonical lane for Telegram and Discord.
- Runtime routing currently allows at most one enabled assistant assignment per human identity.
- ExecAss wakeup already detects unread Agent Mail and can escalate it, but this is an attention/audit path, not direct recipient execution.
- Channel ingest currently uses channel/request model defaults for a run even when a route selects a specific agent; explicit routing must fix this so the selected agent's model/provider defaults win unless the caller explicitly overrides.

## Non-Goals

- Do not replace ExecAss or remove `execass.wakeup`.
- Do not add multi-tenant public SaaS mail delivery.
- Do not add full natural-language routing or fuzzy agent selection.
- Do not add a new external queue system.
- Do not allow tools or API-backed teammates to bypass existing run, provider, rate-limit, auth, and audit guardrails.
- Do not make one external user implicitly share another user's MNO lane or session context.
- Do not introduce new schema unless app-kv metadata cannot satisfy durability/idempotency; prefer existing storage for v1.
- Do not implement fuzzy display-name routing in v1.

## Definitions

- **Agent principal**: the Agent Mail principal id that represents a CarsinOS agent. For first-class agents, the principal id is the agent id unless an explicit alias map is added later.
- **Direct recipient**: a message recipient whose principal id resolves to an active CarsinOS agent.
- **Auto-execution**: CarsinOS creates or reuses a mail-specific session for the recipient agent, writes the mail context as a user message, starts a normal run through existing lane control, and posts the assistant reply back to Agent Mail.
- **Mail delivery worker**: the native gateway path that reacts to newly created Agent Mail messages and schedules recipient auto-execution.
- **Sticky route**: a temporary channel route from a platform conversation + human identity to a selected agent, created by explicit `@agent` syntax.
- **Run-capable teammate**: an agent whose configured provider/model can be executed by the existing run engine. API-backed teammates must be exposed through an existing or future provider/connector adapter before mail auto-execution can call them.

## Runtime Config

Add a small runtime config section, defaulting safe:

```json
{
  "agent_mail_auto_execution": {
    "enabled": false,
    "max_recipients_per_message": 4,
    "max_thread_messages": 12,
    "max_context_chars": 12000,
    "post_failure_notice": false,
    "execass_observer_principal": "execass"
  },
  "explicit_channel_routing": {
    "enabled": true,
    "sticky_enabled": false,
    "sticky_ttl_ms": 1800000
  }
}
```

If the runtime config contract is too risky to widen in the first implementation slice, equivalent constants may be used only if the checklist records a follow-up to expose them through config.

## Functional Requirements

### AMR-1: Direct Agent Mail Recipient Resolution

When Agent Mail creates a message, CarsinOS must inspect the final recipient list.

For each recipient:

- If the recipient resolves to an active agent id, enqueue auto-execution for that agent.
- If the recipient is the sender, skip auto-execution.
- If the recipient is ExecAss, preserve normal delivery and allow ExecAss to auto-execute only if directly addressed.
- If the recipient does not resolve to an agent, keep existing mail behavior and no-op auto-execution.
- If the recipient agent exists but is not run-capable, record a deterministic `recipient_not_runnable` outcome and leave normal mail delivery intact.

Resolution rules:

- Exact `agent_id` match is required for v1.
- Agent ids remain normalized lowercase ids.
- Optional display-name or handle matching is deferred unless implemented with collision-safe validation.

### AMR-2: Mail Auto-Execution Session Contract

For each auto-executed mail recipient, CarsinOS must create or reuse a deterministic session:

```text
agent-mail:thread:<thread_id>:recipient:<agent_id>
```

The session must use the recipient agent id and preserve that agent's model/provider/tool profile/memory binding.

The run prompt must include:

- thread subject
- triggering message id
- sender principal and sender kind
- explicit recipient principal
- recent thread messages, oldest-to-newest, capped by count and character budget
- attachment descriptors only, not raw attachment bytes
- an instruction to answer as the addressed teammate

The prompt must not include unrelated mailbox threads.

The HTTP/MCP send path must not wait for long-running model execution. It should persist the message, emit the normal mail event, and schedule background delivery. A testable internal function may also execute one delivery synchronously for unit tests.

### AMR-3: Reply Back To Agent Mail

When the auto-executed run succeeds and an assistant reply exists:

- CarsinOS posts a new Agent Mail message in the same thread.
- `sender_principal` is the recipient agent id.
- `sender_kind` is `agent`.
- recipients default to the original sender plus ExecAss when ExecAss was not already the sender or recipient.
- metadata records `auto_execution=true`, triggering message id, run id, session id, and recipient agent id.
- CarsinOS acknowledges the triggering message for that recipient after reply creation.

When the run fails, times out, or is blocked:

- CarsinOS must not fake a teammate reply.
- It records an audit/event with the stable failure reason.
- It leaves the message unacked or posts a bounded system failure notice only if configured by the implementation checklist.
- ExecAss wakeup remains able to see unresolved/unacked mail and escalate.

### AMR-4: Anti-Loop And Idempotency

Auto-execution must not recurse forever.

Required guardrails:

- Ignore messages with metadata `auto_execution=true` unless they explicitly name another direct agent recipient.
- Store or derive an idempotency key:

```text
agent-mail-auto:<message_id>:<recipient_agent_id>
```

- Running the delivery worker twice for the same key must not create duplicate runs or duplicate replies.
- The idempotency key must be durable across process restarts, preferably using `app_kv` with a compact JSON status record.
- Cap fanout per message.
- Cap recent thread context.
- Respect the existing single active run per session lane.
- If a recipient's mail session is busy, return a deterministic queued/skipped/busy outcome and leave audit evidence.
- The first implementation may mark busy delivery as `busy_skipped` rather than building a queue, as long as ExecAss visibility remains and no message is lost.

### AMR-5: ExecAss Observer Contract

ExecAss must always have visibility into auto-executed mail without becoming the routine dispatcher.

At minimum:

- auto-execution emits websocket/security/audit events with message id, thread id, recipient agent id, run id if created, and outcome
- unresolved failures remain visible to `execass.wakeup`
- direct mail to ExecAss can still auto-execute ExecAss as a normal addressed agent
- generated recipient replies include the original sender and the configured ExecAss observer principal as recipients when those principals are valid and not duplicates

### AMR-6: Codex-App Skill Contract

Add or document a local Codex skill for sending Agent Mail to a specific CarsinOS agent and waiting for a reply.

The skill should:

- create or reuse a direct thread
- send a message with `recipients=[target_agent_id]`
- poll Agent Mail for replies from that agent
- surface timeout/failure clearly
- avoid direct DB mutation
- use gateway HTTP/MCP APIs only

The skill is optional for runtime tests but required for the user-facing workflow contract.

### CHR-1: Explicit @Agent Routing

Telegram and Discord inbound text must support explicit teammate routing syntax:

```text
@agent-id please review this
@agent_id please review this
```

When the first token resolves to an active agent id:

- route this message to that agent instead of the configured default/lane assignment
- strip the routing token before persisting the user-visible message content, but retain metadata/audit evidence
- use a deterministic direct-agent session key that includes platform, human identity or platform id, conversation id, and agent id
- run through existing provider/model/run guardrails
- use the selected agent's stored `model_provider` and `model_id` as run defaults unless the ingest request explicitly supplies overrides

When the token does not resolve:

- default to the currently configured route or ExecAss/default agent
- record `route_resolution=unresolved`
- do not reject the user's message solely because of an unknown `@token`

### CHR-2: Sticky Route

After a successful explicit `@agent` route, CarsinOS may create a temporary sticky route for the same external conversation.

Sticky behavior:

- default TTL: bounded and short, implementation may choose 30 minutes
- scope: platform + conversation + human identity/platform user + selected agent
- following messages without `@agent` use the sticky agent while the route is valid
- a new explicit `@agent` switches the sticky agent
- a clear command such as `@execass` or implementation-defined reset can return to default route
- sticky routes must be auditable and must not mutate long-term assistant assignments unless a separate explicit admin route does so
- sticky state should live in `app_kv` or another existing runtime state surface with an expiry timestamp; it must not alter `routing.assistant_assignments`

### CHR-3: Memory And Session Safety

Explicit and sticky routing must preserve per-agent and per-lane memory boundaries.

- Do not reuse a session key from another agent.
- Do not route one human into another human's lane.
- If a linked human identity is known, include it in the route/session key.
- If no human identity is known and channel defaults are allowed, use platform user id in the session key.
- MNO context must remain tied to the selected agent/lane, not the previous default agent.

### CHR-4: Compatibility With Existing Mention Gating

Explicit teammate routing does not remove existing Discord/Telegram allowlist and mention-gating protections.

- Unauthorized users remain rejected/ignored as today.
- Group/guild mention requirements still apply before `@agent` is honored.
- The `@agent` token is a target selector, not authentication.

## Implementation Touchpoints

Likely backend files:

- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-storage/src/lib.rs`
- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-channels-discord/src/lib.rs`
- `crates/carsinos-channels-telegram/src/lib.rs`

Likely skill/docs files:

- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_EXECUTION_CHECKLIST.md`
- `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_BLOCKERBOARD.md`
- local Codex skill under `${CODEX_SKILLS_DIR}` or a repo-documented template if runtime skill installation is deferred

## Test Requirements

Backend tests must cover:

- direct mail to one agent creates one run and one reply
- direct mail to two agents fans out boundedly and separately
- mail to non-agent principal keeps existing behavior and creates no run
- mail to sender/self does not auto-execute
- duplicate worker invocation is idempotent
- auto-generated replies do not recurse forever
- failed/busy recipient run leaves audit evidence and does not fake a reply
- ExecAss wakeup still detects unresolved/unacked mail
- Discord `@agent-id` routes to that agent
- Telegram `@agent-id` routes to that agent
- unknown `@token` falls back to default route
- sticky route continues a follow-up turn and expires/resets deterministically
- linked human identity routing does not cross into another user's lane

Validation commands should include at least:

```powershell
cargo fmt --all -- --check
cargo test -p carsinos-gateway agent_mail --locked -- --nocapture
cargo test -p carsinos-gateway channel --locked -- --nocapture
cargo test -p carsinos-storage agent_mail --locked -- --nocapture
```

Broaden to workspace tests if shared protocol/storage contracts change materially.

## Rollout

- Gate auto-execution behind runtime config with a safe default chosen by implementation review.
- Emit clear audit events before enabling broad fanout.
- Keep existing Agent Mail APIs backward-compatible.
- Allow disabling mail auto-execution without disabling Agent Mail itself.

## Backout

- Disable runtime auto-execution config.
- Keep persisted mail data intact.
- Sticky routes are TTL-bound and can be cleared from runtime config/state.
- Existing channel default/lane routing remains the fallback path.

## Stop-Ship Conditions

- Duplicate auto-replies for one message/recipient.
- Infinite or unbounded mail reply loops.
- Unknown `@token` causes message loss.
- A channel user can route into another user's session or memory lane.
- Auto-execution bypasses run lane locks, rate limits, auth roles, or provider/tool guardrails.
- Failed API/LLM teammate replies are presented as successful teammate opinions.
- Explicit `@agent` selects the right session but still runs the wrong model/provider defaults.
r defaults.
