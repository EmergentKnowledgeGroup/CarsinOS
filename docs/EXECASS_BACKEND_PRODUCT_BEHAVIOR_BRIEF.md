# CarsinOS ExecAss Backend Product Behavior Brief

## Status and authority

This brief is subordinate to the locked v1.1 controls:

- `docs/EXECASS_BACKEND_RUNTIME_PRODUCT_CORRECTION_SPEC.md`
- `docs/EXECASS_BACKEND_RUNTIME_PRODUCT_CORRECTION_CHECKLIST.md`
- `docs/EXECASS_BACKEND_RUNTIME_PRODUCT_CORRECTION_BLOCKERBOARD.md`

They are the sole product and implementation authority. This brief explains their intended behavior; it cannot add a policy floor, override an owner instruction, or mark work complete.

## Product contract

CarsinOS supports one human owner, one ExecAss identity, and one CarsinOS instance. The owner supplies intent. ExecAss owns durable coordination. CarsinOS preserves control, truth, and evidence through its existing systems.

An ordinary exact authenticated owner instruction proceeds without permission theater. The backend must not impose morality, content, commercial-purpose, finance, or action-category vetoes. Owner-directed use of an existing external tool is a generic external effect; CarsinOS provides no payment, payee, currency, balance, purchase, monetary allowance, or financial-commitment subsystem.

A resolved dangerous action receives exactly one confirmation that states its concrete consequence. A valid local or authenticated remote owner confirmation makes that unchanged action runnable. The accepted confirmation is durable: it has no use counter or expiry and carries across unchanged replanning, policy revalidation, continuation, restart, bounded retry, and routine occurrence. Only material action drift or an explicit owner amendment, revocation, or cancellation of that action ends it. A dangerous action is never subject to a local-only second challenge, absolute refusal, or repeated confirmation.

## What the backend must make true

- Intake deterministically distinguishes a conversational answer from durable delegated work.
- A Delegation is the durable coordinating record. Existing sessions, runs, jobs, schedules, boards, Agent Mail, teams, tools, connectors, memory, and recovery remain the authoritative machinery beneath it.
- Typed decisions remain inside that same delegation lifecycle. They state why user input is useful and atomically record the result, receipt, outbox event, and zero-or-one applicable continuation.
- Every external or side-effecting action has canonical resolved operands, an idempotency identity, a fenced claim, and objective technical validation. An uncertain external result is reconciled rather than blindly retried.
- Completion means an independently evidenced outcome, not a successful tool or worker report. Terminal reporting stays honest about partial results and uncertainty.
- Direct owner-directed secret delivery may use an existing capable tool, but the raw secret exists only at the minimum transient delivery boundary and never persists in CarsinOS history, receipts, state, logs, outbox, notifications, exports, paths, or recovery artifacts.

## User-visible truth

The sole executive projection is built from durable authoritative state:

- **Needs You** contains typed, actionable attention.
- **In Motion** shows safe active, recovery, and external-wait work.
- **Done** shows evidenced completion or honestly qualified partial completion.
- **Next** shows scheduled occurrences, commitments, deadlines, and follow-ups.
- **Receipts** link each material claim to inspectable evidence.

The projection must not invent activity, hide uncertainty, or become a second task, scheduler, decision, host, or receipt authority. The frontend may use it without exposing internal machinery by default, while deep links retain access to the authoritative records.

## Reliability and trust boundary

CarsinOS records source, authentication, correlation, and idempotency evidence for each ingress. Only authenticated human-controlled ingress creates/amends owner authority or resolves a human decision; runtime, connector, worker, retrieved, model, and child-agent content are evidence only.

The product protects against ordinary faults, duplicate delivery, stale workers, partial commits, and local tampering detectable through receipts and keyed integrity tags. It does not claim to defeat a fully privileged local administrator who can replace binaries, inspect memory, or rewrite data and locally held keys.

## Completion boundary

This correction is not complete or release-ready until the locked checklist is fully evidenced and the locked blockerboard has no open release blocker. This brief must not be used to infer implementation, platform, or release readiness.
