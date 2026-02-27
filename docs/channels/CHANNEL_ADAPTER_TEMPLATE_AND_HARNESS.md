# Channel Adapter Template + Shim Harness (MC-CH-FUT-001)

## Purpose
This document is the implementation template for adding future channels without bespoke gateway wiring. Every new adapter must implement this contract and pass the listed shim harness tests.

## Adapter Contract
### 1. Inbound Routing Decision
Implement a deterministic inbound router that returns one of:
- `Accept` - message should enter gateway run flow.
- `Ignore` - message is valid but not actionable (no mention, unsupported subtype, etc.).
- `Reject` - message is invalid or unauthorized (explicit reason required).

Required output fields:
- `decision`: `accept|ignore|reject`
- `reason`: stable machine-readable reason code
- `session_key`: stable derived key when `decision=accept`
- `transport_message_id`: channel-native message id
- `actor_id`: channel-native actor/user id

### 2. Session Key Rules
Session key derivation must be stable and collision-safe:
- direct/user scope: `<provider>:user:<actor_id>`
- thread scope: `<provider>:thread:<thread_id>`
- channel scope with reply fallback: `<provider>:channel:<channel_id>:reply:<root_msg_id>`

### 3. Outbound Delivery Contract
Adapter must provide:
- `send` (new message)
- `reply` (thread/context reply)
- optional `pin` and `reaction` (if transport supports)

Every method must return:
- `ok`: bool
- `transport_message_id`: optional string
- `retryable`: bool
- `error_code`: optional stable code
- `error_text`: optional short diagnostic

### 4. Retry Semantics
Delivery retries are adapter-owned and deterministic:
- retry only for retryable transport failures (`429`, timeout, transient 5xx)
- no retry for invalid input or auth errors
- exponential backoff with jitter allowed; cap at adapter default timeout

## Shim Harness Requirements
Each new channel adapter must ship all harness tests below.

### Unit Harness
1. Routing matrix:
- Accept case with session key derivation assertion
- Ignore case with stable reason assertion
- Reject case with allowlist/permission denial assertion
2. Approval callback payload round-trip:
- encode/decode identity-preserving
3. Chunking behavior:
- long message split and order preserved
4. Retry policy:
- retryable failure retries, non-retryable does not

### Gateway Shim E2E Harness
1. Inbound accepted message -> session creation/reuse -> run creation
2. Approval-required tool flow round-trip from channel callback
3. Outbound response dispatch emits transport metadata in event/audit trails
4. Adapter reconnect path keeps runtime health green (shim mode)

## Acceptance Gate
A future channel ticket is complete only when:
- unit harness is green
- gateway shim e2e tests are green
- channel runtime status endpoint reports adapter healthy in shim mode
- security audit stream includes adapter action and approval events
