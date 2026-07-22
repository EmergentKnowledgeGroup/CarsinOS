# ExecAss Frontend Integration Handoff

Status: implementation handoff for the versioned ExecAss v1.1 backend contract.

Audience: Mission Control frontend owner.

This document tells the frontend owner where the authoritative contract lives, which transport surfaces to use, where the current Mission Control seams are, and which product behaviors must not be inferred or reinvented.

## Start here

Read these in order:

1. `docs/EXECASS_FRONTEND_EXPERIENCE_BRIEF.md` — the user experience and information hierarchy.
2. `docs/EXECASS_BACKEND_PRODUCT_BEHAVIOR_BRIEF.md` — the product behavior the UI must preserve.
3. `contracts/execass/v1/execass_contract.json` — the machine-readable lifecycle, attention, decision, confirmation, event, error, and ownership rules.
4. `contracts/execass/v1/openapi.json` — the exact HTTP operations, headers, request schemas, response schemas, and error responses.
5. `contracts/execass/v1/schema/` — the checked-in JSON Schema files for every wire DTO.
6. This handoff — the concrete Mission Control connection points and cutover order.

The Rust source of truth for the generated wire contract is `crates/carsinos-protocol/src/execass.rs`. Do not hand-edit generated contract JSON to create frontend-only behavior. Contract regeneration is owned by `crates/carsinos-protocol/src/bin/generate_execass_contract.rs`.

## Product rules the UI must preserve

- Topology is exactly one authenticated owner, one ExecAss, and one CarsinOS instance.
- Ordinary exact owner instructions proceed without permission theater.
- A dangerous or destructive action gets one confirmation that states the consequence.
- Confirmation does not become a veto. A confirmed unchanged action continues.
- Do not introduce spending, purchase, payee, money, finance, compliance-purpose, tenant, organization, role, or second-owner policy into CarsinOS.
- Do not create a second approval engine. Render typed ExecAss attention and resolve the current ExecAss decision.
- Do not ask again merely because a plan, retry, host generation, restart, or routine occurrence changed while the confirmed logical action remained materially unchanged.
- `waiting_for_user` and `waiting_external` are different. Only the former belongs in the primary Needs You surface.
- A claim of completion must link to authoritative receipts/evidence. The summary projection is not a receipt authority.
- Stop state is orthogonal to lifecycle phase. A delegation can retain its phase while run control drains or stops.
- Safe API errors may be shown using `safe_human_message`; never surface internal errors, raw payloads, credentials, tokens, stack traces, or internal paths.

## HTTP API matrix

All routes use bearer authentication. The checked OpenAPI document is authoritative if this table and generated artifacts ever disagree.

| Method | Route | Operation | Frontend purpose |
| --- | --- | --- | --- |
| `POST` | `/api/v1/execass/intake` | `execassIntake` | Submit one outcome-oriented owner request. |
| `GET` | `/api/v1/execass/summary` | `getExecassSummary` | Fetch the sole executive projection for Needs You, In Motion, Done, and Next. |
| `POST` | `/api/v1/execass/summary/ack` | `acknowledgeExecassSummary` | Acknowledge the displayed summary revision. |
| `GET` | `/api/v1/execass/delegations` | `listExecassDelegations` | Page/filter delegations by lifecycle and run-control state. |
| `GET` | `/api/v1/execass/delegations/{delegation_id}` | `getExecassDelegation` | Fetch one authoritative delegation detail. |
| `GET` | `/api/v1/execass/delegations/{delegation_id}/receipts` | `listExecassDelegationReceipts` | Fetch receipt/evidence references for a delegation. |
| `POST` | `/api/v1/execass/decisions/{decision_id}/resolve` | `resolveExecassDecision` | Resolve the exact current typed decision and continue the original work. |
| `POST` | `/api/v1/execass/delegations/{delegation_id}/stop` | `stopExecassDelegation` | Stop one delegation at the safe runtime boundary. |
| `POST` | `/api/v1/execass/delegations/{delegation_id}/resume` | `resumeExecassDelegation` | Resume one stopped delegation from fresh plan/policy snapshots. |
| `GET` | `/api/v1/execass/stop-all` | `getExecassStopAllStatus` | Read global ExecAss run-control status. |
| `POST` | `/api/v1/execass/stop-all` | `engageExecassStopAll` | Request global stop/drain. |
| `POST` | `/api/v1/execass/resume-all` | `resumeExecassAll` | Resume globally stopped work. |
| `GET` | `/api/v1/execass/policy` | `getExecassPolicy` | Read the versioned operational policy. |
| `PUT` | `/api/v1/execass/policy` | `updateExecassPolicy` | Apply an exact owner policy amendment through the canonical authority path. |
| `GET` | `/api/v1/execass/runtime-host` | `getExecassRuntimeHost` | Read desired/actual host state, generation, fencing, and recovery status. |
| `PUT` | `/api/v1/execass/runtime-host` | `configureExecassRuntimeHost` | Configure the single runtime host through the native-owner authority path. |

Every mutation requires an `Idempotency-Key` header matching the request body's `idempotency_key`. Intake, policy update, and runtime-host configuration additionally require `X-ExecAss-Owner-Proof`. Decision resolution carries its exact native decision proof and binding in the request body. The frontend must obtain native proofs through the desktop/native bridge; it must not generate, persist, or approximate signing material in browser storage.

Use `x-request-id` for request correlation. Treat `409` revision/idempotency conflicts as a refetch-and-reconcile condition, not as permission denial. Follow the OpenAPI response table and `schema/api-error.json` for all other error handling.

## Live update contract

ExecAss uses the existing authenticated `/api/v1/ws` transport. It does not create a second socket authority.

After the normal `gateway.status` frame, send:

```json
{
  "type": "execass.v1.resume",
  "client_id": "mission-control-desktop",
  "cursor": 0
}
```

Persist only the last durably handled `global_sequence` for the stable authenticated client identity. Frames use:

- `type: "execass.v1.event"` with the `DurableEventEnvelope` from `schema/durable-event-envelope.json`.
- `type: "execass.v1.summary_refetch_required"` when the cursor is stale, future, mismatched, gapped, or otherwise unsafe.

On `summary_refetch_required`, discard speculative projection changes, refetch `/api/v1/execass/summary`, reconcile the displayed revision, and resume from the server-supported cursor. Deduplicate events by `duplicate_identity`; order them by `global_sequence`. In-memory websocket counters are not authoritative.

The allowed event families are listed in `execass_contract.json`. Use events as invalidation/reconciliation signals. Do not manufacture a second frontend lifecycle reducer that can disagree with the summary/detail projections.

The server implementation and process proof are in:

- `crates/carsinos-gateway/src/main.rs` (`start_execass_stream`, `poll_execass_stream`, and `deliver_execass_replay`).
- `crates/carsinos-gateway/tests/e2e_process.rs` (`execass_durable_outbox_replays_over_authenticated_websocket`).

## Desktop/native connection

Mission Control already has a native runtime bridge in `apps/mission-control/src/lib/runtime.ts` and `apps/mission-control/src-tauri/src/lib.rs`.

The window-close safety flow is already wired:

- Tauri emits `runtime-close-confirmation-required` with the exact challenge binding and consequence.
- The UI presents one confirmation with safe Cancel focus.
- Confirm calls `confirm_runtime_close` with the unchanged binding.
- Cancel calls `cancel_runtime_close_confirmation` and keeps the UI/runtime open.

Keep this separate from an ExecAss work decision. It confirms closing an app-bound runtime; it is not a generic approval surface.

Native owner proof generation for protected HTTP mutations must remain in the native shell/keyring path. The React layer should request a proof for the exact server-derived binding, submit it once, and discard the transient proof material.

## Mission Control cutover points

The current Assistant Desk surface is the frontend seam to replace, not a second product truth to preserve.

Primary files:

- `apps/mission-control/src/lib/api.ts` — add typed ExecAss HTTP adapters and retire the legacy summary call after the new consumer is green.
- `apps/mission-control/src/types.ts` — replace legacy Assistant Desk summary DTOs with generated/validated ExecAss wire types.
- `apps/mission-control/src/features/assistantDesk/useAssistantDeskController.ts` — replace legacy fetching with ExecAss summary/detail/decision orchestration or move the new controller to an explicitly named ExecAss feature module.
- `apps/mission-control/src/features/assistantDesk/AssistantDeskPanel.tsx` — replace the legacy buckets with the experience brief's Needs You, In Motion, Done, and Next presentation.
- `apps/mission-control/src/features/assistant/AssistantChatPage.tsx` — keep outcome intake and the executive projection connected without turning the product into a generic chat wrapper.
- `apps/mission-control/e2e/mockGateway.mjs` — implement the exact versioned fixtures and websocket invalidation behavior; do not keep legacy summary fixtures as a hidden fallback.
- `apps/mission-control/e2e/assistant-desk.spec.ts` — migrate the scenario to the versioned ExecAss contract and one-confirmation continuation.

`docs/EXECASS_EA308_ASSISTANT_DESK_CUTOVER_MAP.json` is the detailed source inventory. Its historical `current_stage: "pre_ea311"` value records when the inventory was frozen; it is not the current implementation status. Use the listed paths and dispositions as the cutover inventory, while this handoff and the checked v1.1 contract control current integration.

Keep Focus, Strategy, Runbooks, receipts, tasks, boards, sessions/runs, Agent Mail, teams, tools, connectors, and memory as deep-linked authoritative or advanced surfaces. Do not duplicate their data into a second ExecAss datastore or make users choose those subsystems before delegating an outcome.

## Recommended frontend implementation order

1. Generate or hand-map TypeScript types directly from the checked schemas and add schema-shaped fixtures.
2. Add a focused `execassApi` adapter for summary, intake, decision resolution, detail, receipts, run control, policy, and runtime-host operations.
3. Add one controller/store that treats summary/detail responses as authoritative and websocket events as invalidation/reconciliation signals.
4. Cut the primary Assistant Desk summary consumer to `/api/v1/execass/summary` with no dual-read fallback.
5. Implement Needs You decisions, including exact revision/challenge rendering and native proof submission.
6. Implement outcome intake, In Motion, Done, Next, receipt deep links, and stop/resume controls.
7. Update the E2E mock and tests to cover reconnect/refetch, idempotent retry, revision conflict, one confirmation, continuation after confirmation, ordinary no-prompt work, external wait, recovery, partial completion, and safe errors.
8. Remove the legacy summary route consumer only after the new production consumer, unit tests, and E2E scenario are green. Retain a transcript/detail path only until the receipts/detail replacement proves equivalent scope and redaction.

## Forbidden shortcuts

- No frontend-only lifecycle, approval category, or danger classifier.
- No legacy/new dual read with precedence guesses.
- No automatic retry of an unknown external effect.
- No repeated confirmation for an unchanged confirmed action.
- No hard refusal after the owner confirms the exact destructive action.
- No localStorage/sessionStorage persistence of owner proofs, decision proofs, gateway bearer tokens, secrets, or raw receipt payloads.
- No fabricated progress, completion, or evidence when the backend reports uncertainty.
- No display of raw internal IDs as the primary user language; preserve IDs for correlation and deep links.

## Validation commands

From the repository root:

```powershell
cargo run -q -p carsinos-protocol --bin generate_execass_contract -- --check
python scripts/validate_execass_contract.py
```

From `apps/mission-control` after frontend changes:

```powershell
npm run typecheck
npm run lint
npm run test:unit
npm run build
npm run test:e2e:core
```

Before pushing frontend changes, perform the repository's required diff-scope audit and desktop/mobile visual verification for every affected shared surface. The production consumer is green only when it executes intake through decision/continuation to terminal evidence against the packaged backend; mock-only success is not sufficient release evidence.
