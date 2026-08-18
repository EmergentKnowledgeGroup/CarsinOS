# Architecture

CarsinOS combines a local Rust control plane with Mission Control, a React and
Tauri operator interface.

```text
Mission Control (React + Tauri)
          │
          │ authenticated HTTP + WebSocket
          ▼
CarsinOS gateway (Rust)
          │
          ├── ExecAss coordination and durable delegations
          ├── agent, provider, connector, and channel routing
          ├── tools, schedules, runbooks, and boards
          ├── confirmations, receipts, and event history
          └── local state, recovery, and runtime control
```

## Core principle

The owner provides intent. ExecAss coordinates the work. CarsinOS keeps the
control, state, and evidence underneath visible and recoverable.

This is deliberately not a multi-user cloud platform. The model is one human
owner, one ExecAss identity, and one CarsinOS instance.

## Main areas

| Area | Role |
| --- | --- |
| `apps/mission-control/` | The web UI and Tauri desktop shell. |
| `crates/carsinos-gateway/` | Authenticated API, WebSocket events, orchestration, and runtime policy. |
| `crates/carsinos-storage/` | SQLite-backed state, receipts, provenance, migrations, and recovery surfaces. |
| `crates/carsinos-protocol/` | Shared request, response, event, and schema contracts. |
| `crates/carsinos-providers/` | Model-provider and authentication adapters. |
| `crates/carsinos-channels-*` | Channel adapters and routing seams. |
| `crates/carsinos-tools/` | Scoped local and network tool execution. |

## The control model

CarsinOS does not turn ordinary owner intent into permission theater. It does
protect important execution boundaries: a dangerous or destructive action gets
one concrete confirmation before it runs. Once the owner confirms an unchanged
action, the decision remains meaningful through normal continuation, bounded
retry, restart, and recovery rather than repeatedly asking the same question.

Mission Control exposes the operational truth needed to supervise that work:
what needs attention, what is moving, what completed, what is scheduled, and
the receipts supporting material claims.

## Local-first by design

The source system is designed around a local gateway, local state, explicit
credentials, and inspectable tool boundaries. External providers or channels
may be connected by the owner, but CarsinOS is not a payment processor,
multi-tenant service, or remote agent marketplace.

See [Security model](SECURITY_MODEL.md) for the relevant operating boundaries.
