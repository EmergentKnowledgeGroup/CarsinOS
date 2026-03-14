# carsinOS Connection Integration Pre-Spec

Generated: 2026-03-10

## Executive Summary

carsinOS is close to ship stage and should treat the next connection work as a controlled integration layer, not an open-ended feature spree.

This document is intentionally lightweight. Its job is to define the shape of the work before research begins on the exact systems, services, and objects the user wants connected.

The goal is to preserve carsinOS's existing product spine while making room for one final integration pass.

## Product Intent

The connection layer should let carsinOS ingest, reflect, and act on selected external systems without creating a second source of truth or a parallel operator workflow.

carsinOS should remain the control surface.

External systems should be treated as:

- inputs
- outputs
- sync targets
- trigger sources
- evidence sources

but not as hidden owners of carsinOS state.

## Goals

This connection work must:

- connect the chosen external systems through a clear gateway-owned integration layer
- preserve canonical ownership inside carsinOS
- surface connection state and failures clearly in Mission Control
- keep auth, secrets, and permission boundaries explicit
- degrade safely when upstream systems are unavailable or partial
- remain testable with deterministic local and mock coverage

## Non-Goals

This pre-spec does not define:

- the final list of systems to connect
- final object mapping for each external service
- UI polish beyond required operator clarity
- broad architecture rewrites unrelated to the chosen connections
- speculative bidirectional sync that cannot be proven safe

## Core Rules

### Rule 1: No Shadow Truth

carsinOS must keep a clearly defined canonical owner for every connected object or field.

If an external system owns a field, carsinOS mirrors it.
If carsinOS owns a field, the integration publishes or syncs it outward.
If ownership is ambiguous, the field stays out of scope until resolved.

### Rule 2: Gateway Owns Integrations

All external connection logic should terminate in gateway-level adapters, contracts, and read/write orchestration.

Mission Control should consume normalized contracts, not vendor-specific payloads.

### Rule 3: Explicit Degraded States

If a connected system is stale, unreachable, rate-limited, partially authorized, or schema-incompatible, carsinOS must show an explicit degraded state rather than inventing healthy sync.

### Rule 4: Secrets Stay Out of Product Surfaces

Tokens, API keys, webhook secrets, and provider credentials must remain in runtime secret/config storage and never leak into frontend state, logs, exports, or checkpoints.

### Rule 5: No Reward Hacking

The implementation must prefer strict sync boundaries, idempotency, and failure clarity over optimistic claims that a connection "works" when only the happy path was tested.

## Scope Shape

The expected implementation shape is:

1. external system adapter(s)
2. protocol contract(s) for normalized requests and responses
3. gateway read/write flows and health state
4. Mission Control operator visibility and controls
5. tests for contract, failure, retry, and degraded-state behavior

The exact connected systems are still `TBD` pending research.

## Canonical Ownership Model

Before implementation begins, each proposed connection must be classified into one of these modes:

- `ingest-only`
  carsinOS reads external facts but does not push state back
- `publish-only`
  carsinOS sends outward updates but does not import authority
- `mirrored-reference`
  carsinOS stores a normalized reference and selected mirrored fields
- `controlled-bidirectional`
  only allowed when field-level ownership and conflict policy are explicit

For every connected entity, the final spec must define:

- canonical system of record
- external identifier
- local identifier
- sync direction
- conflict policy
- stale-data policy
- delete/archive behavior

## Operator UX Requirements

Mission Control should expose only the operator-visible pieces that matter:

- connection health
- last successful sync or publish time
- degraded or blocked reasons
- linked external references
- retry or reconnect actions where appropriate
- safe deep links out to the connected system when useful

The UX must remain additive.

Existing surfaces should not be reworked unless the chosen connection directly belongs there.

## Security And Safety Requirements

The final connection implementation must define:

- auth mechanism per external system
- secret storage path
- allowed scopes and permission minimums
- audit requirements for outbound writes
- replay and duplication protections
- rate-limit handling
- backoff and retry rules
- operator override or kill-switch behavior

## Research Inputs Required

Before the full build spec is finalized, research must answer:

1. Which systems are being connected first, and why now?
2. Is each connection ingest, publish, mirrored reference, or controlled bidirectional?
3. Which exact objects and fields matter for ship stage?
4. What auth model, scopes, limits, and webhook/polling constraints exist?
5. What are the failure modes, and how should Mission Control surface them?
6. What is the minimum viable UX for operators to trust the connection?

## Initial Exit Criteria

This pre-spec is complete enough to move into research when:

- the connection targets are listed
- ownership mode is assigned for each target
- required auth and secret constraints are known
- the first integration slice is scoped to a demoable ship-stage increment

## Open Questions

- Which external systems are mandatory before ship?
- Which ones are nice-to-have but should be deferred?
- Is any bidirectional sync actually necessary for version 1?
- Which existing Mission Control surface should own each connected workflow?
- What evidence will count as "confirmed working" for ship readiness?
