# CarsinOS architecture

CarsinOS combines a local Rust control plane with Mission Control, a React and
Tauri operator interface. The UI is not meant to become a second source of
truth: it reads authoritative gateway projections, uses events to refresh those
projections, lets the owner make explicit decisions, and keeps durable work
visible across restarts and recovery.

~~~text
                    ┌──────────────────────────────────────────┐
                    │ Mission Control                           │
                    │ React UI + optional Tauri desktop shell   │
                    └──────────────────────────────────────────┘
                                      │
                         authenticated HTTP + WebSocket events
                                      │
                    ┌──────────────────────────────────────────┐
                    │ CarsinOS gateway                           │
                    │ local API, coordination, policy, recovery │
                    └──────────────────────────────────────────┘
                       │              │               │
                       ▼              ▼               ▼
              durable storage     tool/runtime     providers + channels
              receipts + state    boundaries       owner-configured paths
~~~

## The central contract

The owner provides intent. ExecAss coordinates the work. CarsinOS keeps the
control, state, and evidence underneath visible and recoverable.

That produces a different shape from a generic chat application:

- The product has one owner and one ExecAss coordinator per local instance.
- Configured workers, models, providers, connectors, and tools can operate
  inside that existing owner authority; they do not become separate owners.
- A request can remain conversational or become a durable delegation with
  intent, criteria, state, decisions, waits, continuation, and receipts.
- An important UI claim should be derived from the relevant authoritative
  projection, not an optimistic local cache or raw event.

Read [Concepts](CONCEPTS.md) for the product model before going deeper into the
implementation map.

## Mission Control: the operator surface

`apps/mission-control/` contains the web application and Tauri desktop shell.
The Glass Office is organized as a data-driven elevator rather than a set of
hardcoded navigation branches:

| Floor | Main idea | Examples |
| --- | --- | --- |
| **The Office** | The owner's working desk | Briefing, delegation intake, Needs You, receipts, Assistant’s Desk |
| **The Window** | A view across the operation | Observed presence and Agent Mail chatter when available |
| **The Trenches** | Durable operational work | Boards, Calendar, Plan, Staff Directory, History & Receipts |
| **The Basement** | The operating machinery | Connectors, models, breakers, event stream, directory, memory, locks, setup, policy |

Each room has a stable ID, route, and declared capabilities. That lets the
elevator resolve hidden or unavailable surfaces honestly, preserves room
identity where multiple rooms share a route, and makes the Office pin/shortcut
mechanism a registry operation instead of a copied data view.

Mission Control also composes a single calm/unknown/incident posture from
authoritative facts already held by its controllers. It does not infer health
from a timer, raw event severity, or a burst of activity. An incident can point
to its owning room; an unavailable destination remains unavailable instead of
becoming a dead navigation action.

## Gateway: the local coordination boundary

`crates/carsinos-gateway/` owns the authenticated HTTP and WebSocket boundary.
It coordinates ExecAss work, exposes durable projections, and hosts the paths
that connect Mission Control to storage, providers, channels, scheduling,
runtime control, and recovery.

At a high level, the gateway is where a request becomes something the system
can track:

~~~text
owner intent
    → intake / delegation
    → plan, work, or decision
    → durable state + events
    → authoritative UI projection
    → receipt / completion / recovery truth
~~~

Foundation admission creates the durable work foundation atomically: authority,
delegation, plan, criteria, optional continuation, lifecycle history, and
outbox state are established together. Intake is idempotent: a matching replay
can return the existing durable foundation, while mismatched reuse conflicts.
Unresolved mechanical structure pauses instead of being silently considered
ready to execute.

## Authority layers

Several authority layers have different jobs. Keeping them distinct avoids
confusing “a request reached the local server” with “the owner authorized an
exact consequential action.”

| Layer | Purpose |
| --- | --- |
| **Transport authentication** | Authenticates gateway HTTP/WebSocket access through static bearer or JWT modes. |
| **Owner authority** | Derives owner authority from authenticated local or allowlisted remote evidence, not a caller-supplied role label. |
| **Owner proof** | Binds ExecAss mutations and decisions to native owner authority; bearer transport auth alone is not sufficient to mint it. |
| **Confirmation custody** | Provides the exact dangerous-action confirmation authority on supported canonical desktop platforms. |
| **Execution policy** | Bounds derived, scheduled, delegated, or unattended work and tool access. |

The product intent remains owner-directed. These layers make authority and
consequence explicit; they are not a generic permission bureaucracy over every
ordinary instruction.

## Confirmation and recovery flow

The confirmation model is intentionally narrow and exact:

1. An ordinary explicit owner instruction proceeds through the ordinary path.
2. A known dangerous ExecAss leaf, or a credible model danger conclusion,
   routes to one concrete confirmation path.
3. The accepted confirmation binds the exact action, target, material/payload,
   tool/version, and stated consequence.
4. An unchanged action can continue; material drift invalidates the earlier
   grant instead of silently applying it to changed work.
5. If recovery cannot proceed safely on its own, the system can stop for an
   owner recovery choice.

There are other decision types in the system, so “one confirmation” should not
be read as “the owner will never see any other question.” It is specifically
the dangerous-action rule.

## Durable state, receipts, and integrity

`crates/carsinos-storage/` provides SQLite-backed persistence for state,
migrations, receipts, provenance, and recovery-facing data. The configured
state root holds the database, attachments, and logs.

Receipts carry structured information about subject, actor, runtime, evidence,
sequence, digest, key generation, integrity tag, and predecessor links. A
receipt append advances both a global journal and the relevant delegation chain
atomically. Receipt integrity provides tamper evidence through an external
anchor and protected key material; it does not claim to defeat a fully
privileged local administrator.

Completion is allowed to be completed, partially completed, or failed based on
material criteria and verifier evidence. If late contrary evidence arrives,
the original terminal receipt is not rewritten; the system records a correction.

## Protocol and UI projection flow

`crates/carsinos-protocol/` owns the shared serialized request, response, and
event contracts. Mission Control consumes a typed contract layer rather than
inventing per-screen payloads.

The gateway exposes authenticated HTTP and a ticketed WebSocket. The live
socket carries ExecAss frames along with other gateway events, but events are
not a second canonical state store: controllers use them to refresh the
authoritative summary/detail reads that drive the UI.

## Providers, tools, connectors, and channels

Focused crates supply the surrounding extension seams:

| Area | Public code location |
| --- | --- |
| Provider and authentication adapters | `crates/carsinos-providers/` |
| Channel adapters | `crates/carsinos-channels-*/` |
| Scoped local and network tool execution | `crates/carsinos-tools/` |
| Runtime-control boundary | `crates/carsinos-runtime-control/` |
| Core shared logic | `crates/carsinos-core/` |
| Command-line entry points | `crates/carsinos-cli/` |

Tools enforce configured filesystem roots, executable names, and network
policy. Provider or channel source presence is not a universal support promise:
each integration still needs real configuration, gateway wiring, and a traced
transport path before it should be represented as available in a local instance.

## Runtime, scheduling, and known boundaries

CarsinOS can expose runtime control, recovery state, scheduled work, and
wakeups through its local gateway and Mission Control surfaces. The installed
desktop runtime path differs from development modes: release desktop builds
own managed sidecar/keyring behavior, while debug Tauri does not start the
managed gateway sidecar.

The architecture is local-first. Public binds, reverse proxies, TLS, remote
access, shared machines, and external deployment are separate design decisions,
not a default inferred from a source checkout.

For the code-level workflow and test map, read [Engineering](ENGINEERING.md).
For operating boundaries, read [Security model](SECURITY_MODEL.md).
