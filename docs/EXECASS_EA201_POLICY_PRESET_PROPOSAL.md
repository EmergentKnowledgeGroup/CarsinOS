# ExecAss EA-201 Operational-Profile Proposal

Status: **SUPERSEDED DESIGN INPUT — NOT IMPLEMENTATION AUTHORITY**
Current authority: locked v1.1 ExecAss specification, checklist, and blockerboard
Applies only as a nonbinding reminder for EA-201 after EA-113 revalidation passes.

## Supersession notice

This document replaces the 2026-07-19 preset proposal. Its former financial fields, category-based approval floors, deny-wins authority model, local-only second challenge, destructive hard locks, and repeated-confirmation behavior are superseded and must not be implemented under renamed fields.

The locked v1.1 controls govern. In particular:

- one owner, one ExecAss, and one CarsinOS instance are supported;
- an authenticated owner instruction authorizes its exact requested or expressly delegated action envelope;
- ordinary exact owner instructions and exact policy amendments proceed without an invented permission step;
- profiles govern derived or unattended operational behavior only and cannot nullify a current exact owner instruction;
- CarsinOS has no financial subsystem or category/morality/commercial-purpose veto;
- a dangerous action gets exactly one concrete-consequence confirmation, and an unchanged accepted action never reprompts across replan, policy revalidation, restart, retry, or routine occurrence;
- direct secret delivery remains non-persistent in CarsinOS surfaces.

## Bounded EA-201 design direction

If and only if EA-113 passes, EA-201 may implement a single canonical representation for `locked_down`, `balanced`, `full_send`, and `custom`. The compiler must make equivalent inputs deterministic and must preserve the locked authority order:

1. Apply stop/revocation state, superseding owner amendments, exact action identity, and objective technical execution validity.
2. Honor the current exact authenticated owner instruction or confirmed amendment.
3. Honor a saved owner instruction within its exact versioned envelope.
4. Apply a profile only to derived or unattended behavior within that envelope.
5. Treat non-human content as planning evidence only.

Profiles may tune objective operational dimensions such as delegation/workspace scope, routine scope, connector or tool identity/version, target, audience, technical resource quota, time, recovery, parallelism, and clarification sensitivity. They may not represent money, payees, currency, balances, purchases, monetary allowances, or financial commitments.

`balanced` may be highlighted during first-run guidance but is never silently stored. An unconfigured first run still accepts and executes an authenticated exact owner request under the base technical and dangerous-action rules.

## Required compiler boundaries

- Canonicalize and freeze resolved action operands before dispatch; unknown, composite, aliased, plugin, shell, or changed-version work pauses only for mechanical resolution or a necessary clarification.
- Derive actor assurance server-side; caller fields, model output, workers, connectors, and retrieved content cannot mint or promote owner authority.
- Use only objective execution/retry facts: capability availability, canonical operands, runtime/platform prerequisites, transaction/fencing validity, idempotency/reconciliation support, and technical resource availability.
- Treat material changes to payload, target, scope, operand, tool/connector version, or consequence as a new action. Internal replan formatting, policy revision, technical recalculation, host generation, retry identity, and expected membership changes inside an unchanged routine envelope are not a new action.
- Keep accepted dangerous-action grants bound to their unchanged action envelope with no expiry or use counter. Only the locked material-drift or explicit-owner-change conditions end them.
- Keep technical resource accounting generic and objectively metered. Do not introduce renamed monetary semantics.

## Required proof before completion

EA-201 remains pending until the locked checklist’s profile-equivalence and owner-authority-precedence property tests pass. The proof must show that profile selection never creates a second authority engine, an action-category veto, a policy-specific confirmation route, or a repeated-confirmation path. It must also show positive ordinary owner-action and policy-amendment cases, the locked one-confirmation flow for dangerous actions, accepted-grant carry-forward, objective-only recovery, no-finance structural scans, and direct-secret non-persistence.

This document neither selects a profile nor authorizes implementation, release, or a standing grant.
