# Engineering CarsinOS

This is the public engineering map for people who want to understand, build,
test, or improve CarsinOS without first excavating the workspace.

CarsinOS is a Rust workspace with a React/Tauri Mission Control frontend. The
most important engineering rule is not a framework preference: preserve the
owner-facing truth. A screen, event, receipt, confirmation, or recovery flow
should represent the authoritative system state rather than inventing a second
one for convenience.

## Repository map

| Location | Responsibility |
| --- | --- |
| `apps/mission-control/` | React web interface, UI tests, browser tests, and Tauri shell |
| `crates/carsinos-gateway/` | Authenticated local API, WebSocket events, coordination, policy, and recovery paths |
| `crates/carsinos-storage/` | SQLite-backed persistence, migrations, receipts, provenance, and durable state |
| `crates/carsinos-protocol/` | Shared API, event, request, response, and schema contracts |
| `crates/carsinos-providers/` | Provider and authentication adapters |
| `crates/carsinos-channels-*/` | Channel-specific adapters and seams |
| `crates/carsinos-tools/` | Explicit local and network tool-execution boundaries |
| `crates/carsinos-runtime-control/` | Runtime-control implementation boundary |
| `crates/carsinos-core/` | Shared domain logic |
| `crates/carsinos-cli/` | Command-line entry points |

Start with [Architecture](ARCHITECTURE.md) for the data-flow view, then come
back here for the change and validation workflow.

## Request and data flow

~~~text
Mission Control
    → authenticated HTTP + ticketed WebSocket
    → CarsinOS gateway
    → protocol DTOs + durable storage
    → configured providers / tools / channels
    → authoritative summary or detail projection
    → Mission Control refreshes its view
~~~

The WebSocket is a live signal and refresh path, not a second canonical client
store. A useful UI change starts by identifying the read model that owns the
fact it will display.

## Development modes are not interchangeable

| Mode | What it is good for | Capability boundary |
| --- | --- | --- |
| Gateway-only source run | Backend, storage, API, and contract work | You configure token and state explicitly. |
| Browser/Vite Mission Control | Frontend iteration and ordinary connected surfaces | It cannot sign native ExecAss owner intake, mutation, or decision proofs. |
| Debug Tauri | Desktop UI development | It does not start the release-managed gateway sidecar. |
| Release desktop path | Packaged runtime behavior | It is where managed sidecar/keyring behavior belongs; do not infer it from debug mode. |

Run the source-development setup from [Getting started](GETTING_STARTED.md).
Keep development state separate from installed state and never commit a local
token, state database, attachment, log, or runtime artifact.

## Choose the smallest truthful change

CarsinOS is cross-cutting, but not every request needs a cross-cutting rewrite.
Before touching code, identify:

1. The owner-visible outcome that must change.
2. The authoritative source of truth that owns it.
3. The smallest implementation surface that can make that source visible or
   correct.
4. The proof that would fail before the change and pass after it.

Examples:

- A stale Office card usually belongs to the controller/projection seam, not a
  new client-side cache.
- A dangerous-action behavior belongs to the exact confirmation and durable
  continuation path, not a generic popup abstraction.
- A room that shares a route with another room still needs its own stable room
  identity for elevator lamp, landing, and pin behavior.
- A visual regression in shared UI needs desktop and narrow-width evidence, not
  only a unit test.

## Contracts and authority

The gateway and Mission Control meet through the protocol crate and typed
frontend contract surface. Treat that boundary as an agreement, not a place to
invent payload shapes casually.

For a behavior change, answer these questions explicitly in the PR description
or design note:

- Which API, event, or durable state owns this fact?
- Is the UI showing a durable truth, a temporary local draft, or an unknown
  state?
- What happens on load failure, identity change, retry, and recovery?
- Does the change make a local UI fact look server-authoritative?
- Does it preserve direct owner intent, exact dangerous-action confirmation, and
  material-drift invalidation?
- If it touches a tool or provider route, what boundary actually constrains it?

Do not use a frontend cache, timer, color, or raw event severity to manufacture
health or completion that the authoritative path has not established.

## Test in layers

Run the smallest test that proves your change first, then the broader gate
proportionate to its blast radius.

### Rust workspace

~~~powershell
# Repository root
cargo fmt --all -- --check
cargo test --workspace --locked
~~~

### Mission Control

~~~powershell
# apps/mission-control
npm run typecheck
npm run lint
npm run test:unit
npm run build
~~~

For a browser-visible core workflow:

~~~powershell
# apps/mission-control
npm run test:e2e:core
~~~

For full browser coverage where the changed path needs it:

~~~powershell
npm run test:e2e:full
~~~

Mission Control also exposes PR and release quality-gate profiles:

~~~powershell
npm run quality:gate:pr
npm run quality:gate:release
~~~

The goal is not to create a ceremonial wall of checks. It is to prove the
changed contract at the lowest level where the behavior is real, then run the
broader protection appropriate to the risk.

## UI work needs visual proof

Mission Control is an operator surface; a correct DOM tree is not enough.

For a changed UI surface:

1. Inspect the changed screen at a normal desktop size.
2. Inspect it at a narrow mobile width.
3. Check keyboard/focus behavior for new dialogs, menus, or navigation.
4. Check console and request failures during the relevant workflow.
5. Keep shared shell changes especially narrow and verify every affected
   surface.

The Glass Office has deliberate laws worth preserving: desktop Office layout
stays bounded, narrow viewports prioritize owner-facing work, hidden/disabled
rooms do not become dead links, and reduced-motion preferences are honored.

## A useful pull request

Keep one PR focused on one user-visible outcome. Include:

- A short description of what changed and why it matters.
- The authoritative behavior or source it relies on.
- Relevant tests and the exact commands run.
- Screenshots or browser evidence for visual changes.
- Explicit security/compatibility notes if permissions, credentials, storage,
  confirmations, runtime control, or data handling changed.

Avoid generated build output, local runtime state, screenshots with private
data, tokens, databases, internal planning material, and broad formatting
churn. If a proposed hardening does not map to a real requirement or reachable
behavior, call it out as a proposal rather than silently expanding the work.

## Extension maturity rule

The workspace contains provider and channel crates, but source presence is not a
support guarantee. Before representing an integration as usable, trace the
actual transport, gateway wiring, configuration path, failure behavior, and
test coverage. Document capabilities individually rather than promising a broad
category because a crate name exists.

## Where to go next

- Product model: [Concepts](CONCEPTS.md)
- System architecture: [Architecture](ARCHITECTURE.md)
- Local setup: [Getting started](GETTING_STARTED.md)
- Security and disclosure expectations: [Security model](SECURITY_MODEL.md)
- Contribution process: [Contributing](../CONTRIBUTING.md)
