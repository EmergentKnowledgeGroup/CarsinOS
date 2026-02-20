# AppDex Strict Implementation Backlog

## Purpose
Drive carsinOS to Mission Control production parity with strict sequencing, modular architecture, and internet-facing production security controls.

## Scope Locks (Non-Negotiable)
1. Channel Phase 1 is only `telegram` + `discord`.
2. All future channels must plug into the same channel adapter contract (no one-off wiring).
3. Full extension/skills architecture is required, not optional.
4. Provider and tool expansion must be modular (adapter/plugin based), not hardcoded growth in `main.rs`.
5. No major Mission Control UI rewrite before backend contracts are stable.
6. No phase after `MC-SEC-*` can be marked release-ready until Security Exit Gate passes.
7. Runtime deployment identities/IDs/tokens/domains/retention targets must be operator-configured, not source-hardcoded.

## Security Deployment Defaults (Locked)
1. Deployment target is internet-facing.
2. Edge identity model is JWT behind API gateway.
3. Tool runtime containment is OS sandbox + allowlisted roots.
4. Release blocks on unresolved critical/high security findings.
5. Consumer OAuth modes remain in production scope but are high-risk controlled.
6. Security checks are mandatory per PR; deep scans run nightly.

## Execution Order (Do Not Reorder)
1. `MC-SEC-*` Security Foundation + Edge Hardening (P0 release blocker).
2. `MC-CONF-*` Setup Wizard + Dynamic Configuration Foundation (P0 operator unblocker).
3. `MC-CH-*` Channel platform + Telegram/Discord production operations.
4. `MC-EXT-*` Extension/plugin runtime and skill system.
5. `MC-TOOL-*` Tool platform hardening + modular expansion.
6. `MC-PROV-*` Provider platform expansion + auth lifecycle hardening.
7. `MC-AUTO-*` Scheduler execution upgrade from stub payloads to real task runs.
8. `MC-FUT-*` Future channel rollout (WhatsApp, Slack, iMessage/BlueBubbles, Signal, Twitch, others).

## Phase 0: Security Foundation + Edge Hardening (P0, Release Blocker)

### MC-SEC-001 - Threat Model + Asset Classification
- Goal: Document trust boundaries, attack paths, and data classes for internet-facing operation.
- Build:
  - STRIDE-style threat model.
  - Data sensitivity map.
  - External dependency trust list.
  - Attacker capability assumptions.
- Depends on: none
- Acceptance:
  - Approved threat model document exists.
  - Every gateway surface is tagged with risk class.

### MC-SEC-002 - Edge Identity and JWT Contract
- Goal: Standardize identity/authn at edge.
- Build:
  - JWT claim contract.
  - Audience/issuer validation requirements.
  - Principal/role propagation contract from API gateway to carsinOS services.
- Depends on: `MC-SEC-001`
- Acceptance:
  - Invalid issuer/audience/signature/expiry are rejected deterministically.
  - Principal and roles are available to policy layer.

### MC-SEC-003 - AuthZ Policy and Role Matrix
- Goal: Enforce role-scoped actions across admin/run/tool/approval/security endpoints.
- Build:
  - Role matrix: `operator_admin`, `operator_readonly`, `automation_runner`, `channel_adapter`, `service_internal`.
  - Endpoint-to-role mapping.
  - Deny-by-default behavior.
- Depends on: `MC-SEC-002`
- Acceptance:
  - Unauthorized role calls fail with stable policy errors.
  - Policy decisions are auditable.

### MC-SEC-004 - Secret and Key Lifecycle
- Goal: Harden secret storage/rotation/revocation.
- Build:
  - Secret source precedence.
  - Rotation cadence.
  - Revocation flow.
  - Bootstrap rules for local keychain and server secret backends.
- Depends on: `MC-SEC-001`
- Acceptance:
  - Rotation/revocation tests pass.
  - Scheduled rotation/revocation job payload modes are validated end-to-end.
  - Non-interactive secret lifecycle drill harness is available and green.
  - Secret material never appears in logs or persisted plaintext payloads.

### MC-SEC-005 - Network Exposure and Transport Security
- Goal: Harden network ingress/egress for internet exposure.
- Build:
  - Explicit public-bind mode controls.
  - TLS termination requirements.
  - Trusted proxy configuration.
  - Strict header forwarding contract.
- Depends on: `MC-SEC-002`
- Acceptance:
  - Untrusted proxy/header spoof attempts are rejected.
  - Secure transport mode test suite passes.

### MC-SEC-006 - Rate Limiting and Abuse Controls
- Goal: Prevent brute force and resource abuse.
- Build:
  - Per-IP, per-principal, and per-endpoint limits with burst controls.
  - Stricter quotas for approval and run endpoints.
- Depends on: `MC-SEC-003`
- Acceptance:
  - Load-abuse tests trigger `429` behavior with deterministic retry headers.
  - Gateway remains stable under abuse patterns.

### MC-SEC-007 - Tool Runtime Containment
- Goal: Enforce OS-level containment for process/web/file tooling.
- Build:
  - Allowed filesystem roots.
  - Executable allowlist.
  - Process/network restriction policy.
  - Sandbox policy object tied to tool risk levels.
- Depends on: `MC-SEC-003`
- Acceptance:
  - Sandbox escape test suite fails closed.
  - High-risk tools require both policy pass and approval pass.

### MC-SEC-008 - Audit Ledger, Retention, and Forensics
- Goal: Create forensic-grade security observability.
- Build:
  - Immutable-style audit event schema.
  - Mandatory correlation IDs.
  - Redaction policy.
  - `90-day` hot retention plus archival policy.
- Depends on: `MC-SEC-003`, `MC-SEC-004`
- Acceptance:
  - Mutation/authz/approval/security events are queryable end-to-end.
  - Actor/action/result chain is complete.

### MC-SEC-009 - Vulnerability and Supply Chain Gate
- Goal: Block high-risk dependencies and known CVEs.
- Build:
  - Per-PR checks: `cargo audit`, advisory policy, dependency policy.
  - Nightly deep scans: SAST, secret scan, container/dependency diff.
  - Severity classification policy.
  - Automation scripts:
    - `scripts/security_pr_gate.sh`
    - `scripts/security_nightly_deep_scan.sh`
- Depends on: none
- Acceptance:
  - CI blocks merge/release on unresolved critical/high findings.
  - Medium findings are tracked with SLA.

### MC-SEC-010 - Security Incident and Kill-Switch Operations
- Goal: Operationalize emergency response.
- Build:
  - Runbooks for auth compromise, key leak, provider abuse, tool abuse, and data exfiltration suspicion.
  - Global/provider/profile kill-switch drill requirements.
  - Drill harness script:
    - `scripts/security_killswitch_drill.sh`
- Depends on: `MC-SEC-003`, `MC-SEC-004`, `MC-SEC-008`
- Acceptance:
  - Tabletop and live drill scenarios pass.
  - Detection/containment metrics are captured for each drill.

### Consumer OAuth High-Risk Policy (Locked)
- Consumer OAuth modes remain supported in production.
- Consumer OAuth profiles must be classified as `high` risk.
- Consumer OAuth profiles must emit operator-visible warning metadata.
- Consumer OAuth paths must be fully auditable.
- Consumer OAuth must be kill-switch controllable at `profile`, `provider`, and `global` scopes.

### Phase 0 Exit Gate (Security Exit Gate)
- Threat model approved and current.
- JWT/authz/rate-limit/sandbox/audit controls active.
- Unresolved critical/high security findings count is zero.
- Incident and kill-switch drills pass.
- Security test suites are green in per-PR and nightly pipelines.

## Phase 0.5: Setup Wizard + Dynamic Configuration Foundation (P0)

### MC-CONF-001 - Configuration Contract v1 (No Hardcoded Runtime Values)
- Goal: Centralize all operator/runtime deployment values in a typed configuration contract.
- Build:
  - Versioned config schema with scopes: `global`, `provider`, `auth_profile`, `channel`, `security`.
  - Required-vs-optional field model with safe defaults and validation rules.
  - Config persistence + migration path for schema changes.
- Depends on: `MC-SEC-002`, `MC-SEC-003`, `MC-SEC-004`
- Acceptance:
  - No source-hardcoded runtime IDs/tokens/domains/issuer/audience values remain in operational paths.
  - Missing required configuration fails closed with deterministic operator-visible errors.

### MC-CONF-002 - Mission Control Setup Wizard (First-Run + Reconfigure)
- Goal: Make production setup fill-in-the-blank and editable without code changes.
- Build:
  - Guided wizard for `R1..R8` operator inputs and deployment-specific channel/provider/security values.
  - Step-level validation and completeness checks.
  - Draft/apply flow with idempotent re-run support.
- Depends on: `MC-CONF-001`
- Acceptance:
  - Fresh installation can become operational via UI/API config flow without editing code.
  - High-risk features remain disabled until required wizard steps are complete.

### MC-CONF-003 - Secret Reference Wiring for Wizard/Config Paths
- Goal: Keep secrets out of plaintext config while retaining operator usability.
- Build:
  - Secret-reference fields and resolution path for tokens/keys.
  - Keychain or secret-backend binding for wizard-provided credentials.
  - Secret redaction and write-path constraints.
- Depends on: `MC-CONF-001`, `MC-SEC-004`
- Acceptance:
  - Wizard/config stores secret references only; no plaintext secret persistence in config records.
  - Secret rotation/revocation remains functional after wizard onboarding.

### MC-CONF-004 - Config Change Governance + Audit
- Goal: Make every configuration mutation auditable and reversible.
- Build:
  - Audit events for config create/update/disable actions with actor, scope, diff hash, and timestamp.
  - Last-known-good snapshot rollback for critical config domains.
  - Kill-switch-aware config mutation guards.
- Depends on: `MC-CONF-001`, `MC-SEC-008`
- Acceptance:
  - All config mutations are queryable in audit history.
  - Rollback restores valid prior config state without service restart.

### MC-CONF-005 - Hardcoded Value Elimination Audit + CI Guard
- Goal: Prevent hardcoded runtime values from re-entering the codebase.
- Build:
  - Repository-wide hardcoded-value audit (`issuer`, `audience`, IDs, tokens, domains, retention targets, role owner IDs, channel IDs).
  - Allowlist file for intentional constants with owner + expiry metadata.
  - CI check to fail PRs that introduce new disallowed hardcoded runtime values.
- Depends on: `MC-CONF-001`, `MC-SEC-009`
- Acceptance:
  - Baseline hardcoded-value audit report exists and is actionable.
  - CI blocks new disallowed hardcoded runtime-value patterns.

### MC-CONF-006 - Global Runtime Value Externalization (Audit Remediation Batch A)
- Goal: Remove global runtime/deployment fallbacks from source defaults.
- Build:
  - Move Numquam base URL and integration principal identity defaults to runtime config/wizard fields.
  - Move gateway bind and GUI gateway base URL defaults to config contract-backed values.
  - Move OpenAI OAuth redirect URI fallback to runtime config-backed value.
- Depends on: `MC-CONF-001`, `MC-CONF-002`, `MC-CONF-005`
- Acceptance:
  - No global deployment URL/identity/bind value is source-hardcoded in operational paths.
  - Wizard/config can fully define these values without env-only dependency.

### MC-CONF-007 - Provider Endpoint Externalization (Audit Remediation Batch B)
- Goal: Eliminate provider local-endpoint hardcoded defaults from runtime execution paths.
- Build:
  - Move vLLM and Ollama fallback API base URLs to provider/runtime config fields.
  - Ensure provider auth profile precedence remains deterministic after migration.
- Depends on: `MC-CONF-001`, `MC-CONF-005`
- Acceptance:
  - Provider endpoint defaults are config-driven.
  - Regression tests verify provider selection still works without source edits.

### MC-CONF-008 - Security Policy Externalization (Audit Remediation Batch C)
- Goal: Move high-impact security/runtime policy defaults to operator-controlled config.
- Build:
  - Move tool network allowlist defaults from source literals to runtime security config.
  - Move integration principal identifiers used for audit provenance into security config.
- Depends on: `MC-CONF-001`, `MC-CONF-002`, `MC-CONF-005`, `MC-SEC-003`
- Acceptance:
  - Security egress and identity policy values are operator-configurable and audited.
  - Internet-facing mode rejects missing required values for these fields.

### MC-CONF-009 - Channel Runtime Default Externalization (Audit Remediation Batch D)
- Goal: Remove implicit source defaults for channel runtime behavior.
- Build:
  - Move channel default run model provider/model to config wizard-required fields for production mode.
  - Replace static channel tool-provider fallback with runtime-derived allowlist from enabled channels.
- Depends on: `MC-CONF-001`, `MC-CONF-002`, `MC-CONF-005`, `MC-CH-001`
- Acceptance:
  - Channel behavior defaults are explicit in config and operator-visible.
  - No hardcoded channel runtime provider/model fallback remains for release-ready mode.

### Phase 0.5 Exit Gate (Configuration Readiness Gate)
- Setup wizard can fully configure runtime without source edits.
- Required deployment values are config-driven and validated.
- Hardcoded-value CI guard is active and green.
- Config mutation audit and rollback paths are verified.
- Audit remediation tickets (`MC-CONF-006..009`) are complete or explicitly deferred with owner + target milestone.

## Phase 1: Channels First (P0 Functional)

### MC-CH-001 - Channel Adapter Contract v1
- Goal: Formal channel runtime contract for inbound/outbound/status/approval actions.
- Build:
  - Create shared traits/interfaces in `carsinos-core`.
  - Move Telegram/Discord helper logic behind adapter boundary.
  - Add adapter lifecycle (`start`, `stop`, `health`, `reconnect`).
- Depends on: `MC-CONF-001`, `MC-SEC-003`
- Acceptance:
  - Gateway can register channel adapters without channel-specific branches in route handlers.
  - Channel adapters are swappable via config/registry.

### MC-CH-002 - Channel Runtime Manager
- Goal: Always-on channel orchestration service.
- Build:
  - Supervisor for adapter startup/restart/backoff.
  - Per-channel status snapshots + health probes.
  - Structured events for connection state changes.
- Depends on: `MC-CH-001`, `MC-CONF-004`
- Acceptance:
  - Channel runtime manager survives adapter crashes and reconnects automatically.
  - Status endpoint exposes adapter state and last error.

### MC-CH-010 - Telegram Production Connector
- Goal: Real Telegram inbound/outbound operation.
- Build:
  - Bot token auth, long-polling first, webhook second.
  - Mention/allowlist/DM policy enforcement.
  - Thread/topic/session mapping + chunked outbound delivery.
- Depends on: `MC-CH-002`, `MC-CONF-002`, `MC-CONF-003`, `MC-SEC-003`
- Acceptance:
  - Message in Telegram triggers run and returns reply.
  - Recovery after network interruption verified in e2e tests.

### MC-CH-020 - Discord Production Connector
- Goal: Real Discord guild/DM/thread operations.
- Build:
  - Gateway event intake and outbound send/reply handling.
  - Mention gating, allowlist, thread/channel session mapping.
  - Chunking and delivery retry behavior.
- Depends on: `MC-CH-002`, `MC-CONF-002`, `MC-CONF-003`, `MC-SEC-003`
- Acceptance:
  - Guild and DM roundtrip works with stable session mapping.
  - Long replies are chunked and delivered reliably.

### MC-CH-030 - Channel Approval Actions
- Goal: Resolve approvals directly from Telegram/Discord interactions.
- Build:
  - Callback/action bindings for approve/deny.
  - Idempotent approval handling and audit trail metadata.
- Depends on: `MC-CH-010`, `MC-CH-020`, `MC-SEC-008`
- Acceptance:
  - Approval actions from channels resolve gateway approvals with exact run linkage.

### Phase 1 Exit Gate
- Telegram + Discord pass 7-day soak with reconnect resilience.
- Approval action flow passes e2e tests for both channels.
- No channel-specific logic leaked outside adapter/runtime manager layers.
- Security Exit Gate remains green.

## Phase 2: Extension + Skills Architecture (P0)

### MC-EXT-001 - Plugin Runtime Foundation
- Goal: Safe plugin loading/registration lifecycle.
- Build:
  - Plugin manifest schema.
  - Loader + registry + capability declarations.
  - Versioned plugin API surface.
- Depends on: `MC-CH-001`, `MC-SEC-003`
- Acceptance:
  - Plugin can register tools/hooks/providers/channels through registry only.

### MC-EXT-002 - Hook Bus + Lifecycle Points
- Goal: Controlled extension hooks across run lifecycle.
- Build:
  - Hook points for run start/end, before/after tool call, before/after compaction.
  - Priority ordering and failure isolation.
- Depends on: `MC-EXT-001`
- Acceptance:
  - Hook failure cannot crash gateway.
  - Hook execution is observable and auditable.

### MC-EXT-003 - Skills System v1
- Goal: Workspace skill loading equivalent (safe layer).
- Build:
  - Skill discovery from configured dirs.
  - Enable/disable controls and metadata index.
  - Prompt/context skill injection policy.
- Depends on: `MC-EXT-001`
- Acceptance:
  - Skills can be discovered, toggled, and applied per run context.

### MC-EXT-004 - Extension Security Controls
- Goal: Bound extension risk.
- Build:
  - Capability allowlist and permission policy checks.
  - Reserved command/method protection.
  - Audit logs for denied extension actions.
- Depends on: `MC-EXT-002`, `MC-EXT-003`, `MC-SEC-003`, `MC-SEC-008`
- Acceptance:
  - Unauthorized plugin actions are blocked with deterministic errors.

### Phase 2 Exit Gate
- Plugins and skills operate via formal registry/policy.
- Security policy tests pass for permission denial and hook isolation.
- Security Exit Gate remains green.

## Phase 3: Tool Platform Expansion (P1)

### MC-TOOL-001 - Tool Registry Refactor
- Goal: Move from hardcoded tool parsing to registry-driven tool execution.
- Build:
  - Tool registry in gateway runtime.
  - Tool definition metadata (`risk_level`, `requires_approval`, `timeouts`).
  - Backward compatibility for existing `tool.*` commands.
- Depends on: `MC-EXT-001`, `MC-SEC-007`
- Acceptance:
  - New tool can be added without editing core run loop logic.

### MC-TOOL-002 - Tool Hardening Pass
- Goal: Production hardening of existing tools.
- Build:
  - Consistent timeout/truncation/error envelopes.
  - Unified structured tool telemetry.
  - Concurrency limits and cancellation semantics.
- Depends on: `MC-TOOL-001`, `MC-SEC-006`, `MC-SEC-007`
- Acceptance:
  - All built-in tools return normalized outputs and errors.

### MC-TOOL-003 - Channel Action Tooling (Telegram/Discord)
- Goal: Expose safe channel actions as tools.
- Build:
  - send/reply/pin/reaction (where supported) via adapter APIs.
  - Permission policy checks.
- Depends on: `MC-CH-010`, `MC-CH-020`, `MC-TOOL-001`, `MC-SEC-003`
- Acceptance:
  - Tool calls can trigger channel actions with full auditability.

## Phase 4: Provider Platform Expansion (P1)

### MC-PROV-001 - Provider Adapter Contract v2
- Goal: Standardize provider adapters for modular expansion.
- Build:
  - Capability metadata (streaming, tools, json mode, vision, max context).
  - Unified error taxonomy and retry classes.
- Depends on: `MC-SEC-003`
- Acceptance:
  - Provider-specific failures normalize to stable error classes.

### MC-PROV-002 - Auth Lifecycle Hardening
- Goal: Strong auth/profile lifecycle under load.
- Build:
  - Token refresh policy.
  - Profile health scoring + fallback ordering.
  - kill-switch enforcement tests.
- Depends on: `MC-PROV-001`, `MC-SEC-004`
- Acceptance:
  - Expired auth profiles refresh or fail over deterministically.

### MC-PROV-010 - Provider Expansion Pack 1
- Goal: Add first modular provider expansion set.
- Build:
  - `openrouter`, `ollama`, `vllm` adapters (minimum).
  - Config and auth wiring through provider registry.
- Depends on: `MC-PROV-001`, `MC-PROV-002`, `MC-SEC-003`
- Acceptance:
  - Each new provider can run via same adapter contract and auth profile flow.

## Phase 5: Automation Execution Upgrade (P1)

### MC-AUTO-001 - Job Payload to Real Task Execution
- Goal: Replace scheduler stub payload executor with real task execution path.
- Build:
  - Job payload schema for actual run requests.
  - Execute run engine from scheduler with persisted lifecycle.
- Depends on: `MC-TOOL-001`, `MC-PROV-001`, `MC-SEC-003`
- Acceptance:
  - Scheduler job executes real model/tool workflow, not synthetic `noop/fail`.

### MC-AUTO-002 - Delivery Targets + Run Outcome Routing
- Goal: Scheduled runs can deliver to selected channel targets.
- Build:
  - Delivery policy with fallback and retry.
  - Deterministic eventing for started/succeeded/failed/delivered.
- Depends on: `MC-AUTO-001`, `MC-CH-010`, `MC-CH-020`, `MC-SEC-008`
- Acceptance:
  - Jobs can deliver output to Telegram/Discord with auditable outcomes.

## Phase 6: Future Channel Queue (P2, Modular Only)

### MC-FUT-010 - WhatsApp Adapter
- Depends on: `MC-CH-001`, `MC-CH-002`, `MC-SEC-003`

### MC-FUT-020 - Slack Adapter
- Depends on: `MC-CH-001`, `MC-CH-002`, `MC-SEC-003`

### MC-FUT-030 - iMessage/BlueBubbles Adapter
- Depends on: `MC-CH-001`, `MC-CH-002`, `MC-SEC-003`

### MC-FUT-040 - Signal Adapter
- Depends on: `MC-CH-001`, `MC-CH-002`, `MC-SEC-003`

### MC-FUT-050 - Twitch Adapter
- Depends on: `MC-CH-001`, `MC-CH-002`, `MC-SEC-003`

### MC-FUT-900 - Additional Channels (Backlog)
- Candidates: Matrix, LINE, Mattermost, Google Chat, MS Teams, Feishu, IRC, Zalo.
- Rule: each must implement adapter contract and pass the same soak/test gates.

## Global Quality Gates (All Phases)

### Security Gate 0 (Release Blocker, Must Pass First)
1. Threat model approved.
2. JWT/authz/rate-limit/sandbox/audit controls active.
3. Unresolved critical/high security findings count is zero.
4. Incident kill-switch drills pass.
5. Security test suites are green in per-PR and nightly pipelines.

### Functional and Architectural Gates
1. Feature-flag every major capability.
2. Add e2e tests for every new adapter/provider/tool before merge.
3. Keep migration-safe DB changes with forward/backward compatible migrations.
4. Maintain run/approval audit trail continuity across all changes.
5. No direct business logic inside transport handlers; all logic in services/registries.
6. New runtime identifiers/secrets/integration IDs must enter through config contract/wizard paths, never hardcoded in source.
7. Hardcoded-value audit gate must pass before release candidate tagging.

## Sprint Cadence
1. Sprint S0: `MC-SEC-001`, `MC-SEC-002`, `MC-SEC-003`, `MC-SEC-004`, `MC-SEC-005`
2. Sprint S1: `MC-SEC-006`, `MC-SEC-007`, `MC-SEC-008`
3. Sprint S2: `MC-SEC-009`, `MC-SEC-010`, Security Gate 0 validation
4. Sprint S3: `MC-CONF-001`, `MC-CONF-002`, `MC-CONF-003`
5. Sprint S4: `MC-CONF-004`, `MC-CONF-005`, Phase 0.5 gate validation
6. Sprint S5: `MC-CONF-006`, `MC-CONF-007`, `MC-CONF-008`, `MC-CONF-009`
7. Sprint A: `MC-CH-001`, `MC-CH-002`
8. Sprint B: `MC-CH-010`
9. Sprint C: `MC-CH-020`, `MC-CH-030`
10. Sprint D: `MC-EXT-001`, `MC-EXT-002`
11. Sprint E: `MC-EXT-003`, `MC-EXT-004`
12. Sprint F: `MC-TOOL-001`, `MC-TOOL-002`, `MC-TOOL-003`
13. Sprint G: `MC-PROV-001`, `MC-PROV-002`, `MC-PROV-010`
14. Sprint H: `MC-AUTO-001`, `MC-AUTO-002`
15. Sprint I+: `MC-FUT-*` channel queue in business-priority order.
