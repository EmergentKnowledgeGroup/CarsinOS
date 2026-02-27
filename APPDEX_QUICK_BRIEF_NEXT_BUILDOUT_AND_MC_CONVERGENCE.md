# AppDex Execution Brief: CarsinOS Buildout (Now) + Mission Control Convergence (Later)

Date: 2026-02-26
Owner: AppDex
Audience: Operator + Runtime Engineering

## Executive Intent
CarsinOS has the OpenClaw-style core loop working end-to-end (runs, approvals, scheduler, Telegram/Discord listeners, guardrails). The next wave is **extensibility without fragility**:
1. Make plugins real (executable, isolated, observable), not just registry metadata.
2. Make providers/tools modular (additive) without growing `main.rs`.
3. Keep future channels on-demand and strictly behind the same safety/approval/audit model.

Mission Control convergence is a **separate** project track. Do not let UI work block runtime hardening.

## Locked Decisions (Operator)
These are binding constraints for the next buildout.

Plugins:
- Plugin artifacts are **local path only** (no remote fetch/distribution in v1).
- Integrity is **SHA256 required** at install/update.
- Plugin breaker: **3 consecutive failures** then disable for **15 minutes** (cooldown).
- Runner: **hybrid** (subprocess default; daemon allowlist-only).
- Daemon allowlist source: **runtime config list** (operator configurable).
- Rollback depth: **single previous version**.
- Plugin scope v1: **hooks + plugin-defined tools** (both in scope now).
- Plugin tool default policy: **high risk + approval required**.
- Tool name collisions across core + plugins: **reject install/update**.

Providers/Tools/Channels:
- Provider modularity: **all providers at once** (conformance harness is non-optional).
- Tool capability contract: **single global capability list endpoint**.
- Future channel template: **no concrete channel target** until demanded.

## What Changed Since The Previous Brief (Gaps Closed)
This brief rewrites and expands the earlier “quick brief” because it was under-specified in a few key areas:
1. **Plugin “execution depth” was undefined** (packaging, sandbox, failure model, compatibility, rollback semantics).
2. **Provider/tool modularity lacked a concrete adapter contract + conformance tests**, so “modular” could regress into hardcoded growth.
3. **Future channels lacked a repeatable template + acceptance harness**, so each channel risked bespoke wiring.
4. **Mission Control convergence lacked concrete deliverables and sequencing**, risking scope bleed into runtime work.

## Baseline (Already Verified Green)
Do not redo this work unless fixing regressions.
- Autonomy guardrails (budgets, breakers, lane locks, scheduler single-instance lock).
- Scheduler job execution (real `session.run`, plus `heartbeat.run` no-tools contract).
- Telegram + Discord:
  - Listener loops running in transport mode.
  - Ingest -> run -> outbound reply path.
  - Channel tool actions (`send/reply/pin/reaction`) with audit.
- Extensions/skills control plane:
  - Plugin registry lifecycle endpoints (install/update/rollback).
  - Skill registry load + enable/disable.

## Track A (Now): CarsinOS Runtime Extensibility, Safely

### A0) Non-Negotiables (Do Not Break)
1. No prompt-only safety. Guardrails must remain **runtime-enforced**.
2. No secrets in plaintext config or logs. Use secret refs + secret store.
3. No PII in sample fixtures, docs, or reports.
4. Every new “extension” surface must still produce:
   - deterministic audits (`/api/v1/security/audit`),
   - stable reason codes on deny/stop,
   - and regression tests.

### A1) Plugins v2 Execution Depth (Packaging + Runner + Health)

#### Summary
Build "real plugins": executable, observable, rollbackable, and failure-isolated so they cannot destabilize the gateway.

#### A1.1 Manifest schema v2 (backward-safe)
Extend the plugin manifest with:
- `artifact.exec_kind`: `subprocess|daemon`
- `artifact.command`, `artifact.args[]`
- `artifact.sha256` (required)
- `compat.min_gateway_version`
- `permissions.allowed_roots[]`
- `permissions.network_policy`: `deny_all|allowlist`
- `permissions.network_allowlist[]`
- `limits.timeout_ms`, `limits.max_output_chars`

Install/update validation rules (reject on failure):
- Local artifact path only; SHA256 must match file content.
- Tool capability names must be unique across core + all plugins (collision rejects).
- `exec_kind=daemon` requires plugin id present in runtime config allowlist.

#### A1.2 Bundle layout + atomic install/update/rollback (single-depth)
Filesystem layout under `state_dir/plugins/`:
- `bundles/<plugin_id>/<version>/` (artifact + manifest copy + metadata)
- `active/<plugin_id>.json` (active manifest)
- `pointers/<plugin_id>.json` (active version pointer + previous pointer)

Atomicity rule:
- write bundle dir first
- write/rename `active/<plugin_id>.json` last
- update pointer file last

Rollback rule:
- single previous version only
- swap active manifest back to previous version and audit the rollback

#### A1.3 Runner contract (`carsinos-plugin-runner v1`, hybrid)
Transport:
- `subprocess`: stdin JSON request (single object), stdout JSON response (single object), stderr allowed (captured/truncated).
- `daemon`: NDJSON over stdin/stdout with `request_id` correlation; serialize one in-flight request per plugin daemon.

Request envelope:
- `contract_version`, `request_id`, `kind` (`hook|tool`), `plugin_id`, `deadline_ms`, `permissions`, `limits`, `payload`

Response envelope:
- `contract_version`, `request_id`, `status` (`ok|error|deny`), `result`, `error_code`, `error_message`, `metrics` (`duration_ms`, `output_chars`, `timed_out`)

Strict stdout rule:
- any non-JSON stdout is a hard failure (`EXT_PLUGIN_OUTPUT_INVALID`)

#### A1.4 Gateway invoke path + safety controls
Hooks:
- First supported hook points: `run.start`, `run.end`, `tool.before`, `tool.after`.
- Invoke matching enabled plugins with per-plugin timeout + output cap.
- Failure isolation: plugin failure cannot fail the run by default; it emits events/audit and updates plugin breaker state.

Plugin-defined tools:
- Dynamically register plugin tools from manifests into the tool registry.
- Default policy: `requires_approval=true`, `risk_level=high`.
- Tool execution invokes plugin runner `kind=tool` and returns a tool-result envelope.

Hybrid runner policy:
- default `subprocess`
- daemon starts lazily on first invocation; restart on exit; kill+restart on per-request timeout; auto-disable on breaker trip

#### A1.5 Plugin health + breaker
State per plugin:
- `enabled`, `faulted`, `disabled_until_ms`, `consecutive_failures`, `last_error_code`, `last_error`, `last_success_ms`

Policy:
- on each failure: increment failures
- on success: reset failures
- at 3 consecutive failures: disable for 15 minutes; emit event + audit

Status surfaces:
- extend `/api/v1/status` response with plugin health summary
- add `GET /api/v1/extensions/plugins/status` returning per-plugin state (operator_readonly allowed)

#### A1.6 Observability + audit requirements
Events:
- `extension.plugin.invoked`, `extension.plugin.succeeded`, `extension.plugin.failed`, `extension.plugin.disabled`

Security audit actions:
- install/update/rollback already present; extend with invoke/disable audit entries

Stable error taxonomy (examples):
- `EXT_PLUGIN_DISABLED`, `EXT_PLUGIN_TIMEOUT`, `EXT_PLUGIN_OUTPUT_INVALID`, `EXT_PLUGIN_EXEC_FAILED`, `EXT_PLUGIN_POLICY_DENY`, `EXT_PLUGIN_SCHEMA_INVALID`

#### A1.7 Tests + benchmark gates
Unit tests:
- manifest validation (sha mismatch, collision rejection, daemon allowlist enforcement)
- runner envelope parsing/validation
- breaker/disable behavior deterministic

Gateway E2E:
- install plugin + verify hook invoked
- plugin hang -> timeout -> gateway remains responsive
- plugin crash -> failure recorded -> after 3 failures plugin disabled (15m)
- plugin tool invocation requires approval and produces tool-result envelope

Benchmarks:
- baseline gateway benchmark remains green
- add "hook invoke overhead" benchmark: default budget is +2ms p95 under stub runner

### A2) Providers and Tools: True Modularity + Conformance

#### Problem To Prevent
“Modular” cannot mean “add more `match` arms in code until it becomes OpenClaw again.”

#### Provider Work (Next)
1. Define/lock a provider adapter contract:
   - capabilities, supported tool mode, max context, error class mapping, usage accounting, optional cost model.
2. Move provider registration away from `main.rs` growth:
   - prefer config-driven enable/disable and per-provider parameters.
3. Add a provider conformance suite:
   - shared test harness that runs each provider through known success/failure paths.
   - confirms budgets/kill-switch/breakers apply uniformly.

Acceptance:
- Adding a new provider does not require touching run-loop logic.
- Provider conformance suite fails if guardrails are bypassed.

#### Tool Work (Next)
1. Treat tool policies as first-class data:
   - `risk_level`, `requires_approval`, sandbox roots/binaries/network rules, timeouts.
2. Add a tool capability endpoint:
   - allows Mission Control and operators to see what exists and what is enabled.
3. Conformance tests:
   - tool output truncation, timeout, approval gating, sandbox deny paths.

Acceptance:
- New tool cannot bypass approval or sandbox policy.
- Tool policies are observable and test-covered.

### A3) Future Channels: On-Demand Modules (No One-Off Wiring)

#### Policy
- Telegram + Discord stay the only production channels for this wave.
- WhatsApp/Slack/iMessage/Signal/Twitch are implemented only when demanded.
- Every channel must use the same safety model:
  - allowlists/mention gating,
  - approvals,
  - audit trail,
  - and guardrails.

#### Standard Channel Adapter Contract (Must Hold)
For any new channel:
1. Transport client with retry semantics.
2. Listener loop (or webhook intake) that calls the same ingest path.
3. Deterministic routing (`Accept|Ignore|Reject`) with reason strings.
4. Stable session key mapping rules.
5. Outbound actions: at minimum `send` + `reply`; optional `pin`/`reaction` if channel supports.
6. Approval action payload round-trip.

#### Acceptance
- New channel ships with:
  - unit tests for routing/session key,
  - a gateway-level e2e test in shim mode,
  - and no bypasses around approvals/audit.

### A4) Definition Of Green (Track A)
- `cargo test` (workspace) is required before merge.
- Any new extension/provider/channel must include deny-path tests.

## Track B (Later): Mission Control Convergence (Separate Stream)

### B0) Rules
1. This is a UI/UX + integration project, not a runtime refactor.
2. Keep existing Mission Control cockpit/layout engine; add new “spec surfaces” as pages/widgets.
3. If implementing UI/UX: use `frontend-design` skill and keep Mission Control checkpoints updated per its SOP.

### B1) Inputs
- Mission Control baseline: `missioncontrol/docs/MISSION_CONTROL_V2_DESIGN_PLAN.md`.
- External concept artifacts (screenshots/spec notes). Treat as IA/UX inspiration, not runtime contract.

### B2) Deliverables (Planning First)
1. **Surface map**
   - Decide canonical surfaces: Tasks, Content Pipeline, Approvals, Council, Calendar, Memory/Journal, Team/Agents.
   - Explicitly decide what is dropped (example: “Office view” if no longer desired).
2. **Data contract + API map**
   - For each surface, list:
     - data objects,
     - current API sources (CarsinOS gateway vs existing MC storage),
     - missing endpoints/events.
3. **Migration matrix**
   - `existing MC` -> `target surface` -> `work required`.
4. **Phase plan**
   - Phase 1: wire CarsinOS status/events into MC widgets.
   - Phase 2: build Content Pipeline + Tasks surfaces.
   - Phase 3: calendar/scheduled jobs + memory + team roster.

Acceptance:
- No big-bang rewrite. Each phase ships independently.
- Each phase has validation commands recorded (typecheck/lint/test/build).

## Ticket Seed List (AppDex)
Use these as the next executable backlog. Keep Track A and Track B separate.

Track A:
1. `MC-EXT-NEXT-001` Plugin manifest v2 + packaging layout + sha verification + collision checks + atomic install/update/rollback.
2. `MC-EXT-NEXT-002` Runner contract + subprocess invoke path + hook execution + breaker/health + events/audits + tests.
3. `MC-EXT-NEXT-003` Daemon-mode support + runtime config allowlist + restart/timeout semantics + tests.
4. `MC-TOOLS-NEXT-001` Plugin tools registration/execution + tool capabilities endpoint + tool conformance harness.
5. `MC-PROV-NEXT-001` Provider contract freeze + provider conformance harness + refactor boundaries (no run-loop edits to add provider).
6. `MC-CH-FUT-001` Channel adapter template + shim e2e harness for future channels (no concrete channel target yet).

Track B (planning only):
1. `MC-UX-CONV-000` Surface map + API map + migration matrix + phase plan.

## Execution Sequence + PR Chunking (CodeRabbit-friendly)
1. PR-A: Plugin manifest v2 + packaging layout + sha verification + collision checks + updated install/update/rollback.
2. PR-B: Runner contract + subprocess invoke path + hook execution + breaker/health + events/audits + tests.
3. PR-C: Daemon-mode support + runtime config allowlist + restart/timeout semantics + tests.
4. PR-D: Plugin tools registration/execution + tool capabilities endpoint + tool conformance harness.
5. PR-E: Provider contract freeze + provider conformance harness + refactor out of gateway run loop.
6. PR-F: Channel adapter template + harness.

## Gates / Checkpoints (Workflow)
- Update `runtime/checkpoints/LATEST.md` + `runtime/checkpoints/LATEST.json` at:
  - phase start
  - post-green validation
  - PR open
  - post-merge
- Regressive gates at end of each PR:
  - `cargo test --workspace --locked`
  - `cargo test -p carsinos-gateway --test e2e_process`
  - `cargo test -p carsinos-gateway --test benchmark_process -- --nocapture`
- Audit/logging: every plugin/tool/provider mutation and every deny/disable must be queryable via `/api/v1/security/audit`.

## Owner Input Required
- None for this implementation plan; locked decisions above are sufficient.
- Optional later: pick a concrete future-channel target when starting the first real new channel implementation (not needed to build the template/harness).
