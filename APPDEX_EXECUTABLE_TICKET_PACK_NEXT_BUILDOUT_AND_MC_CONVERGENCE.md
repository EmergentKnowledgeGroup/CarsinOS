# AppDex Executable Ticket Pack: Next Buildout (Runtime Extensibility) + Mission Control Convergence (Planning Only)

Date: 2026-02-26
Owner: AppDex
Audience: Runtime Engineering + Operator

## Purpose
Convert the "Next Buildout" brief into an implementation-ready backlog: IDs, dependencies, acceptance criteria, concrete API/IO contracts, tests, gates, and PR chunking.

This pack is optimized for:
- speed/functionality/stability over feature parity
- clear blast-radius control (breakers, cooldowns, rollbacks)
- regression-first delivery (E2E + benchmark gates)

## Scope
Track A (build now):
- Plugins v2 execution depth: packaging + hybrid runner + hooks + plugin-defined tools + health/breakers + observability.
- Tools: global capability surface + conformance harness, including plugin tools.
- Providers: contract freeze + conformance harness so adding providers does not bloat the run loop.
- Future channels: template + harness only (no new channel target).

Track B (plan later):
- Mission Control convergence planning and sequencing only.

## Locked Decisions (Binding)
Plugins:
- Plugin artifacts: local path only.
- Integrity: SHA256 required at install/update.
- Breaker: 3 consecutive failures, 15 minute cooldown.
- Runner: hybrid (subprocess default; daemon allowlist-only).
- Daemon allowlist source: runtime config list (operator configurable).
- Rollback depth: single previous version.
- Plugin scope v1: hooks + plugin-defined tools.
- Plugin tool defaults: high-risk + approval required.
- Tool name collisions: reject install/update.

Providers/Tools/Channels:
- Provider modularity: all providers at once (conformance harness non-optional).
- Tool capabilities: single global capability endpoint/list.
- Future-channel template: no specific channel target.

## Non-Goals (This Pack)
- Plugin marketplace/distribution.
- Remote artifact fetch for plugins.
- In-process plugin execution or dynamic linking.
- "Untrusted third-party code" hard security without OS-level isolation.
  - NOTE: out-of-process runner + policy envelopes provide stability and auditability, not a full sandbox. For untrusted code, add OS sandbox/containerization as a later hardening ticket.

## Definitions
- "Plugin tool": a tool whose implementation is provided by a plugin runner.
- "Hook": a lifecycle callback invoked by gateway code, calling into a plugin runner.
- "Hybrid runner": per-plugin choice of `subprocess` (spawn per invoke) or `daemon` (long-lived worker).

## Public Interfaces (Freeze / Add)
### Plugin Manifest v2 (conceptual)
The manifest schema is extended (backward-safe) with:
- `artifact.exec_kind`: `subprocess|daemon`
- `artifact.command`, `artifact.args[]`
- `artifact.sha256` (required)
- `compat.min_gateway_version`
- `permissions.allowed_roots[]`
- `permissions.network_policy`: `deny_all|allowlist`
- `permissions.network_allowlist[]`
- `limits.timeout_ms`, `limits.max_output_chars`
- `tools[]` optional: tool declarations (name + description + risk default policy)

### Plugin Runner Contract v1 (IO contract)
Transport:
- subprocess: stdin JSON request, stdout JSON response (single object each)
- daemon: NDJSON request/response with `request_id`; one in-flight request at a time

Request envelope fields:
- `contract_version`: `"carsinos.plugin_runner.v1"`
- `request_id`: string
- `kind`: `"hook" | "tool"`
- `plugin_id`: string
- `deadline_ms`: u64
- `permissions`: object (see below)
- `limits`: object (timeout/output caps)
- `payload`: object (kind-specific)

Response envelope fields:
- `contract_version`
- `request_id`
- `status`: `"ok" | "error" | "deny"`
- `result`: object|null
- `error_code`: string|null
- `error_message`: string|null
- `metrics`: object

Strict stdout rule:
- any non-JSON stdout is a hard failure (`EXT_PLUGIN_OUTPUT_INVALID`)

### Tool Capabilities Endpoint (new)
`GET /api/v1/tools/capabilities`
- Returns the union of:
  - core tools
  - plugin tools (from enabled plugin manifests)
- Must be stable and safe for Mission Control consumption.

Fields:
- `tool_name`
- `origin`: `core|plugin:<plugin_id>`
- `risk_level`
- `requires_approval`
- `timeout_ms`
- `enabled`
- `sandbox`: `allowed_roots[]`, `network_policy`, `network_allowlist_count`

### Stable Error Codes (additive)
Plugin errors (examples; extend as needed but keep stable):
- `EXT_PLUGIN_DISABLED`
- `EXT_PLUGIN_TIMEOUT`
- `EXT_PLUGIN_OUTPUT_INVALID`
- `EXT_PLUGIN_EXEC_FAILED`
- `EXT_PLUGIN_POLICY_DENY`
- `EXT_PLUGIN_SCHEMA_INVALID`
- `EXT_PLUGIN_SHA_MISMATCH`
- `EXT_PLUGIN_TOOL_NAME_COLLISION`
- `EXT_PLUGIN_DAEMON_NOT_ALLOWLISTED`

Tool capability errors:
- `TOOL_CAPS_UNAVAILABLE`

## Storage Layout (append-only)
Under `state_dir/plugins/`:
- `bundles/<plugin_id>/<version>/`
- `active/<plugin_id>.json`
- `pointers/<plugin_id>.json` (active + previous only)

## Observability (required)
### Events
- `extension.plugin.invoked`
- `extension.plugin.succeeded`
- `extension.plugin.failed`
- `extension.plugin.disabled`

### Audit ledger
Ensure these are queryable via `/api/v1/security/audit`:
- plugin install/update/rollback (already present)
- plugin invoke (hook/tool)
- plugin disabled (breaker)

### Logging
Every plugin invocation log line must carry:
- `request_id`
- `plugin_id`
- `kind` (hook/tool)
- `exec_kind` (subprocess/daemon)
- `latency_ms`
- `status`
- `error_code` (if any)

## Testing and Gates (release discipline)
### Required validation commands per PR
- `cargo test --workspace --locked`
- `cargo test -p carsinos-gateway --test e2e_process`
- `cargo test -p carsinos-gateway --test benchmark_process -- --nocapture`

### Minimum new tests to add (Track A)
Unit tests:
- manifest v2 validation: sha mismatch, tool collision, daemon allowlist
- runner contract parsing and strict stdout handling
- breaker state machine: 3 failures => disabled_until + cooldown

E2E tests (process):
- install plugin and verify hook invoked
- plugin hang => timeout and gateway remains responsive
- plugin crash => failure recorded, breaker trips after 3 failures
- plugin tool requires approval and returns tool-result envelope

Benchmarks:
- add "hook invoke overhead" benchmark; keep p95 delta within configured budget (default: +2ms p95 under stub runner)

## PR Chunking (CodeRabbit-friendly)
Chunking is mandatory; do not open a single mega-PR for this pack.

1. PR-A: packaging + manifest v2 + sha + collisions + atomic install/update/rollback
2. PR-B: runner contract + subprocess invoke + hooks + breaker + events/audit + tests
3. PR-C: daemon mode + allowlist + supervisor/restart semantics + tests
4. PR-D: plugin tools + capabilities endpoint + tool conformance harness
5. PR-E: provider contract freeze + provider conformance + boundaries refactor
6. PR-F: channel adapter template + shim harness

## Ticket Index (Track A)
### Epic MC-EXT-NEXT: Plugins v2 Execution Depth
- `MC-EXT-NEXT-001` Manifest v2 + validation (local artifact + sha256 + tool collision + daemon allowlist)
- `MC-EXT-NEXT-002` Packaging layout + atomic install/update + single-depth rollback pointer
- `MC-EXT-NEXT-003` Plugin runner contract v1 (JSON/NDJSON) + strict stdout rule
- `MC-EXT-NEXT-004` Subprocess runner invoke path + timeouts/output caps + env sanitization
- `MC-EXT-NEXT-005` Hook execution wiring (`run.start|run.end|tool.before|tool.after`)
- `MC-EXT-NEXT-006` Plugin health/breaker + cooldown + status surfaces
- `MC-EXT-NEXT-007` Daemon runner support + allowlist enforcement + supervisor semantics
- `MC-EXT-NEXT-008` Plugin tool execution integration (approval-required) + error mapping
- `MC-EXT-NEXT-009` Plugin E2E + benchmark coverage

### Epic MC-TOOLS-NEXT: Tool Capability Surface + Conformance
- `MC-TOOLS-NEXT-001` `GET /api/v1/tools/capabilities` endpoint + tests
- `MC-TOOLS-NEXT-002` Tool conformance harness (core + plugin tools)

### Epic MC-PROV-NEXT: Provider Contract Freeze + Conformance (All Providers)
- `MC-PROV-NEXT-001` Provider adapter contract freeze (capabilities/errors/usage/cost fail-closed)
- `MC-PROV-NEXT-002` Provider conformance harness (success/retry/terminal + budgets/kill-switch/breakers)
- `MC-PROV-NEXT-003` Refactor boundaries: adding provider does not require run-loop edits

### Epic MC-CH-FUT: Future Channel Template (No Target Yet)
- `MC-CH-FUT-001` Channel adapter template + shim E2E harness

## Ticket Details (Implementation-Ready)

### MC-EXT-NEXT-001 - Manifest v2 + Validation (P0)
Goal:
- Extend manifest schema to support hybrid runner + tool declarations + permission/limit envelopes.

Dependencies:
- none (additive only; backward-safe parsing required)

Build:
- Add manifest v2 fields (see "Public Interfaces").
- Enforce validation at install/update:
  - local artifact path only
  - sha256 required and must match content
  - tool name collisions reject install/update
  - `exec_kind=daemon` requires id allowlisted in runtime config

Acceptance:
- Invalid manifests fail deterministically with stable error codes.
- Existing v1 manifests continue to load (defaults applied).

Tests:
- unit tests for each rejection case + backward-compatible parse.

Gates:
- required validation commands (see "Testing and Gates").

### MC-EXT-NEXT-002 - Packaging Layout + Atomic Lifecycle + Single Rollback (P0)
Goal:
- Standardize on-disk bundle layout and ensure install/update/rollback are atomic and audit-visible.

Dependencies:
- `MC-EXT-NEXT-001`

Build:
- Implement filesystem layout under `state_dir/plugins/`:
  - bundles/<id>/<version>/
  - active/<id>.json
  - pointers/<id>.json (active + previous only)
- Atomic rule: finalize active manifest last (rename).
- Rollback: only one previous retained; swap pointer and emit audit.

Acceptance:
- No partial state on crash during install/update.
- Rollback works and is recorded (audit + event).

Tests:
- integration tests around install/update/rollback and crash-safety simulation (best-effort).

Gates:
- required validation commands.

### MC-EXT-NEXT-003 - Plugin Runner Contract v1 (P0)
Goal:
- Freeze the runner IO contract used by gateway for both hooks and tools.

Dependencies:
- `MC-EXT-NEXT-001`

Build:
- Define request/response envelope structs and serde rules.
- Implement strict stdout parser: non-JSON => `EXT_PLUGIN_OUTPUT_INVALID`.
- Define stable error codes and mapping.

Acceptance:
- Contract is documented and versioned; gateway rejects mismatched version.

Tests:
- unit: round-trip parse, invalid stdout, missing fields, unknown status.

Gates:
- required validation commands.

### MC-EXT-NEXT-004 - Subprocess Invoke Path (P0)
Goal:
- Invoke plugin runner as subprocess (spawn-per-call) with hard limits.

Dependencies:
- `MC-EXT-NEXT-003`
- `MC-EXT-NEXT-002` (for artifact location and active manifest)

Build:
- Spawn runner with:
  - per-invoke timeout (manifest limits + global ceiling)
  - stdout max chars + truncation behavior
  - stderr capture + truncation
  - env sanitization (explicit env allowlist only)
  - stable working directory (plugin bundle dir)
- Record metrics and emit events/audit on each invoke.

Acceptance:
- Hanging plugin times out and cannot stall gateway.
- Crashing plugin cannot crash gateway.

Tests:
- E2E: install stub plugin that hangs and verify timeout.
- E2E: stub plugin that prints garbage => output invalid error.

Bench:
- baseline benchmarks remain green.

Gates:
- required validation commands.

### MC-EXT-NEXT-005 - Hook Execution Wiring (P0)
Goal:
- Implement hooks: `run.start`, `run.end`, `tool.before`, `tool.after`.

Dependencies:
- `MC-EXT-NEXT-004`

Build:
- Define hook payload shapes (minimal, stable).
- Invoke enabled plugins on hook fire; isolate failures (do not fail run by default).

Acceptance:
- Hooks fire deterministically; failures recorded but do not cascade.

Tests:
- E2E: stub plugin records hook invocation count and returns ok.

Gates:
- required validation commands.

### MC-EXT-NEXT-006 - Plugin Health + Breaker + Status (P0)
Goal:
- Provide runtime breaker state with 3-failure disable and 15m cooldown.

Dependencies:
- `MC-EXT-NEXT-004`

Build:
- Maintain per-plugin health state (in memory + persisted metadata if needed):
  - consecutive failures, disabled_until, last error, last success
- Policy: 3 consecutive failures => disabled for 15 minutes.
- Add status surfaces:
  - extend `/api/v1/status`
  - add `GET /api/v1/extensions/plugins/status`

Acceptance:
- Breaker disables plugin deterministically; cooldown enforced.
- Operator can see why a plugin is disabled.

Tests:
- unit: breaker state machine
- E2E: crash 3 times => disabled; invoke while disabled => `EXT_PLUGIN_DISABLED`

Gates:
- required validation commands.

### MC-EXT-NEXT-007 - Daemon Runner Support + Allowlist (P1)
Goal:
- Add daemon-mode plugin execution with strict allowlist and supervisor semantics.

Dependencies:
- `MC-EXT-NEXT-003`
- `MC-EXT-NEXT-001` (daemon allowlist validation)

Build:
- Runtime config: add list of daemon-allowed plugin ids.
- Supervisor:
  - lazy-start on first invoke
  - serialize requests (1 in flight)
  - restart on exit
  - kill+restart on per-request timeout
  - breaker trip disables daemon as well

Acceptance:
- Non-allowlisted daemon plugins cannot start.
- Daemon restarts are visible and auditable.

Tests:
- E2E: daemon allowlist enforcement
- E2E: daemon exits => restarted on next invoke

Gates:
- required validation commands.

### MC-EXT-NEXT-008 - Plugin-Defined Tools (P0)
Goal:
- Enable plugin tools as first-class tools with approval-required default policy.

Dependencies:
- `MC-EXT-NEXT-004`
- `MC-TOOLS-NEXT-001` (capability surface should include plugin tools)

Build:
- Register tool entries from manifests into tool registry at runtime.
- Default policy: `risk=high` + `requires_approval=true`.
- Execution path:
  - approval gate
  - invoke plugin runner kind=tool
  - map result into tool output envelope

Acceptance:
- Plugin tools cannot bypass approvals.
- Tool name collision prevents install/update.

Tests:
- E2E: plugin tool invocation requires approval
- E2E: denial path is auditable and emits stable reason code

Gates:
- required validation commands.

### MC-EXT-NEXT-009 - Plugin E2E + Bench Coverage (P0)
Goal:
- Lock regression coverage so plugin execution does not regress gateway stability.

Dependencies:
- `MC-EXT-NEXT-004..008`

Build:
- Add E2E tests listed in "Testing and Gates".
- Add benchmark coverage for hook overhead.

Acceptance:
- E2E + benchmarks are required gates before merge.

### MC-TOOLS-NEXT-001 - Tool Capabilities Endpoint (P0)
Goal:
- Add `GET /api/v1/tools/capabilities` returning union of core + plugin tools.

Dependencies:
- plugin tool registry exists (`MC-EXT-NEXT-008`) OR ship core-only first, then extend (preferred: include plugin tools in the same PR-D).

Build:
- Add endpoint + protocol type and tests.

Acceptance:
- Output is stable and filterable by origin/risk/enabled.

### MC-TOOLS-NEXT-002 - Tool Conformance Harness (P1)
Goal:
- Add shared conformance tests for tools (core + plugin).

Dependencies:
- `MC-TOOLS-NEXT-001`

Build:
- Harness cases: timeout, truncation, sandbox deny, approval gating.

Acceptance:
- Tool regressions fail CI deterministically.

### MC-PROV-NEXT-001 - Provider Contract Freeze (P0)
Goal:
- Freeze provider interface so adding providers is purely additive and conformance-tested.

Dependencies:
- none (but coordinate with existing provider interfaces in `carsinos-providers`)

Build:
- Contract includes:
  - capabilities (streaming/tools/json/vision)
  - error taxonomy + retryability
  - required usage metrics for budgets
  - cost model behavior: if USD budget configured but cost cannot be computed => fail closed with budget error

Acceptance:
- All providers implement the same interface and emit normalized errors/usage.

### MC-PROV-NEXT-002 - Provider Conformance Harness (P0)
Goal:
- Prevent provider regressions and enforce budgets/kill-switch/breakers uniformly.

Dependencies:
- `MC-PROV-NEXT-001`

Build:
- Harness: success, retryable failure, terminal failure, budget stop, kill-switch, breaker open/skip/reset.

Acceptance:
- Adding a provider requires implementing fixtures; run loop untouched.

### MC-PROV-NEXT-003 - Refactor Boundaries (P0)
Goal:
- Ensure provider-specific logic does not bloat gateway run loop.

Dependencies:
- `MC-PROV-NEXT-001`

Build:
- Move provider-specific logic behind the provider trait/module boundaries.
- Gateway consumes only frozen interface and conformance-proven behavior.

Acceptance:
- Adding provider does not require editing run-loop logic.

### MC-CH-FUT-001 - Channel Template + Harness (P1)
Goal:
- Document and codify a channel adapter pattern to avoid bespoke wiring.

Dependencies:
- none

Build:
- Template doc + skeleton:
  - routing Accept/Ignore/Reject with reason
  - session key rules
  - transport retry semantics
  - shim E2E harness

Acceptance:
- A new channel can be implemented by filling in template + tests.

## Ticket Index (Track B - Planning Only)
- `MC-UX-CONV-000` Mission Control convergence planning: surface map + API map + migration matrix + phase plan.

