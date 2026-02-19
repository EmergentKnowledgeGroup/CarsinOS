# SECURITY_HARDENING_PROGRAM.md

## 1. Purpose, Scope, and Non-Goals

### Purpose
Define the security baseline and execution program for internet-facing carsinOS deployments. This document is the decision-complete implementation contract for the security hardening wave and maps all controls to AppDex `MC-SEC-*` tickets, tests, and release gates.

### In Scope
- Edge identity/authentication contract for JWT + API gateway model.
- Service-side authorization policy and deny-by-default enforcement.
- Secret/key lifecycle controls and operational rotation/revocation.
- Network exposure controls (public bind mode, transport constraints, trusted proxy rules).
- Abuse protection and rate-limit controls.
- Tool runtime containment (filesystem/process/network).
- Audit/forensics event model, retention, and evidence requirements.
- Security CI/CD gates and unresolved finding policy.
- Incident runbooks and kill-switch operations.

### Non-Goals
- SOC 2/ISO certification program rollout in this phase.
- Full enterprise SIEM/SOAR platform integration (only required event export contracts are defined).
- Mobile endpoint hardening and BYOD policy management.
- Replacing existing product roadmap phases (`MC-CH/EXT/TOOL/PROV/AUTO/FUT`); this program gates release-readiness.

## 2. System Trust Boundaries and Threat Model Summary

### Primary Trust Boundaries
1. Internet client -> API gateway edge.
2. API gateway -> carsinOS gateway service.
3. carsinOS gateway -> provider APIs (OpenAI/Anthropic/etc.).
4. carsinOS gateway -> tool runtime/process execution.
5. carsinOS gateway -> persistence (`sqlite`, logs, audit stream).
6. carsinOS gateway -> channel adapters (Telegram/Discord and future adapters).
7. carsinOS gateway -> external memory services (Numquam HTTP/MCP).

### High-Risk Assets
- Auth credentials and refresh tokens.
- Provider/API secrets and setup tokens.
- User conversation content and memory embeddings.
- Approval decisions and audit trails.
- Tool execution inputs/outputs and filesystem access paths.

### Threat Model Method
- STRIDE-style analysis by trust boundary.
- Required output artifacts:
  - Threat register (`id`, `boundary`, `vector`, `impact`, `likelihood`, `mitigations`, `owner`).
  - Surface inventory with risk classification (`low`, `medium`, `high`, `critical`).
  - Mitigation mapping to `MC-SEC-*` tickets.

### Baseline Adversary Assumptions
- External attacker can reach internet-exposed endpoints.
- External attacker can replay requests and spoof headers.
- Compromised low-privileged token may be available to attacker.
- Malicious prompt or payload may attempt tool escape/path traversal.
- Dependency/supply-chain advisories may emerge post-release.

## 3. Identity and Access Architecture (JWT Edge Model + Role Matrix)

### Edge Identity Model (Locked)
- API gateway validates JWT signature and trusted issuer metadata.
- carsinOS gateway re-validates critical claims and trusted edge headers.
- Requests without valid identity context are rejected.

### JWT Claims Contract (Required)
- `iss` (required): exact allowlisted issuer.
- `aud` (required): must include service audience.
- `sub` (required): principal identifier.
- `exp` (required): token expiry; deny expired tokens.
- `iat` (required): issued-at; enforce max token age policy.
- `jti` (required for replay protection): unique token identifier.
- `roles` (required): list of role identifiers.
- `scope` (optional): additional authorization narrowing.

### Clock Skew Policy
- Maximum accepted skew: 60 seconds.
- `exp` beyond skew window: reject.
- `iat` in future beyond skew window: reject.

### Role Matrix (Locked)
- `operator_admin`: full configuration/auth/profile/kill-switch/security admin actions.
- `operator_readonly`: read-only operational visibility, no mutations.
- `automation_runner`: execute approved run/scheduler paths only.
- `channel_adapter`: channel-specific send/receive/approval action subset.
- `service_internal`: internal service calls only; blocked from operator admin actions.

## 4. AuthZ Decision Model and Endpoint Classification

### AuthZ Defaults
- Deny by default for all mutating endpoints.
- Explicit allowlist mapping from endpoint/action to role set.
- Missing role claims -> hard deny.
- Unknown role strings -> hard deny.

### Endpoint Classes
1. Security-admin endpoints: require `operator_admin`.
2. Auth/profile mutations: require `operator_admin`.
3. Approval resolve actions: require `operator_admin` or explicit delegated role.
4. Run execution and scheduler mutation: require `operator_admin` or `automation_runner`.
5. Read-only status/metrics/list endpoints: `operator_admin` + `operator_readonly`.
6. Channel action pathways: constrained by `channel_adapter` and target policy.

### Auth Context Propagation Contract
Required normalized context fields from edge gateway to carsinOS service:
- `principal_id`
- `role_set` (array)
- `auth_method` (`jwt`)
- `token_id` (JWT `jti`)
- `session_id` (if available)
- `request_id`

### Stable AuthZ Error Taxonomy
- `AUTH_REQUIRED`
- `AUTH_INVALID`
- `AUTH_EXPIRED`
- `AUTH_FORBIDDEN`
- `AUTH_ROLE_MISMATCH`
- `POLICY_DENY`

## 5. Secret/Key Lifecycle and Rotation Policy

### Secret Storage Policy
- Secrets stored only in approved secret stores (keychain/server backend).
- Persisted config/database stores only non-secret metadata and secret references.
- Logs must never include secret material.

### Source Precedence
1. Runtime explicit secret backend.
2. Keychain references.
3. Environment bootstrap only for initial provisioning (no plaintext persistence).

### Rotation and Revocation
- Rotation target:
  - provider tokens/keys: every 30 days or provider TTL, whichever is stricter.
  - signing/verification keys: every 90 days.
- Revocation:
  - immediate deny for revoked token IDs (`jti`) and revoked profiles.
  - emergency rotation runbook must complete within on-call window.

### Required Test Coverage
- rotated secret invalidates previous credential paths.
- revoked profile cannot execute runs.
- refresh-token rollover updates references without plaintext persistence.

## 6. Tool Runtime Containment Model

### Containment Defaults
- Tool runtime executes in restricted OS sandbox profile.
- Filesystem access restricted to allowlisted roots.
- Executable invocation restricted to allowlisted binaries/command classes.
- Network egress policy defaults to deny unless tool explicitly whitelisted.

### Sandbox Policy Schema (Frozen Contract)
- `risk_level` (`low|medium|high|critical`)
- `allowed_roots` (path list)
- `allowed_binaries` (command list)
- `network_policy` (`deny_all|allowlist`)
- `max_timeout_ms`
- `max_output_bytes`
- `requires_approval` (bool)

### Enforcement Rules
- Policy evaluation occurs before tool execution.
- High-risk operations require both policy pass and explicit approval state.
- Sandbox/policy violation returns stable error and audit event.

## 7. Abuse Protection and Rate-Limiting Model

### Required Limits
- Per-IP limit bucket.
- Per-principal limit bucket.
- Per-endpoint limit bucket.
- Stricter policy for `/runs`, `/approvals/*/resolve`, auth mutation endpoints.

### Burst and Sustained Controls
- Burst window: short-term spike control.
- Sustained window: long-running abuse throttling.
- Backoff hints included in response.

### `429` Contract (Frozen)
Response envelope fields:
- `error_code`: `RATE_LIMITED`
- `scope`: `ip|principal|endpoint`
- `retry_after_seconds`
- `request_id`

## 8. Audit/Telemetry Schema and Retention Policy

### Security Audit Event Envelope (Frozen)
- `event_id`
- `request_id`
- `timestamp_utc`
- `principal`
- `action`
- `resource`
- `decision`
- `reason`
- `transport`
- `status`
- `error_code` (optional)
- `correlation_id`

### Mandatory Audited Actions
- Auth success/failure and authz denies.
- Auth profile create/update/disable.
- Kill-switch changes (profile/provider/global).
- Approval request/resolve decisions.
- Tool execution start/finish/fail and policy denies.
- Security policy/configuration mutations.

### Retention Policy
- Hot searchable retention: 90 days.
- Archive retention: required beyond hot window per operations policy.
- Redaction policy applies to secrets and oversized payload fields.

## 9. Consumer OAuth High-Risk Policy and Controls

### Policy
Consumer OAuth is allowed in production but always treated as high-risk.

### Required Controls
- Risk classification must be `high`.
- Operator-visible warning metadata mandatory at profile creation/update and run selection.
- Full audit trail mandatory for all consumer OAuth profile operations and run usage.
- Kill-switch compatibility required at all scopes:
  - profile
  - provider
  - global
- Fallback behavior must respect risk policy and kill-switch state.

## 10. Incident Response Runbooks and Kill-Switch Operations

### Mandatory Runbooks
1. Token/auth compromise.
2. Secret/key leak.
3. Provider abuse/spend anomaly.
4. Tool runtime abuse or sandbox violation event.
5. Suspected data exfiltration.

### Kill-Switch Drill Requirements
- Drill cadence: quarterly minimum + on major auth/runtime changes.
- Drill scenarios:
  - profile scope isolation,
  - provider-wide stop,
  - global emergency halt.
- Operational harness:
  - `scripts/security_killswitch_drill.sh` executes deterministic drill test cases and writes machine-readable artifacts under `runtime/security/reports/`.
- Required outputs:
  - detection timestamp,
  - containment completion time,
  - blast radius confirmation,
  - remediation actions.

## 11. Security Test Strategy (Per-PR and Nightly)

### Per-PR Mandatory Checks
- Unit and integration security suites.
- JWT validation and authz denial tests.
- Rate limit behavior tests.
- Tool sandbox deny/allow tests.
- Secret lifecycle rotation/revocation suites (API + scheduled job modes).
- `cargo audit` policy check.
- Secret leak scan on diff.
- Automation entrypoint:
  - `scripts/security_pr_gate.sh` (release-blocking gate script).

### Nightly Deep Scans
- Full SAST run.
- Dependency and advisory diff analysis.
- Secret scanning over full repository.
- Container/image and build-surface checks (if applicable).
- Extended abuse/load security tests.
- Automation entrypoint:
  - `scripts/security_nightly_deep_scan.sh` (nightly deep scan orchestrator).
  - `scripts/security_secret_lifecycle_drill.sh` (non-interactive secret rotation/revocation drill harness).

### Evidence Outputs
- Machine-readable test reports.
- Finding severity summary by category.
- Comparison against previous run for regression detection.
- Required artifact location:
  - `runtime/security/reports/`

## 12. Release-Blocking Criteria and Exception Policy

### Hard Release Blocks
- Any unresolved `critical` or `high` security finding.
- Security Gate 0 incomplete.
- Missing mandatory audit event coverage for mutation/authz paths.
- Failed kill-switch drill acceptance.

### Medium/Low Findings
- `medium`: allowed only with owner, SLA, and tracked remediation ticket.
- `low`: backlog permitted with owner assignment.

### Exception Policy
- Exception requires explicit risk acceptance by designated owner.
- Exception includes expiry date and rollback/mitigation plan.
- Exceptions never bypass unresolved critical findings.

## 13. Control-to-Ticket Traceability Matrix (`MC-SEC-*`)

| Control Domain | Ticket(s) | Primary Output |
| --- | --- | --- |
| Threat model and asset classification | `MC-SEC-001` | Threat register + surface risk classes |
| JWT edge identity contract | `MC-SEC-002` | Claims/issuer/audience contract + validation tests |
| Role-based authz policy | `MC-SEC-003` | Role matrix + endpoint policy map + deny tests |
| Secret/key lifecycle | `MC-SEC-004` | Rotation/revocation policy + scheduled job cadence + drill harness + secret handling tests |
| Network and transport hardening | `MC-SEC-005` | Public bind/TLS/proxy contract + spoofing tests |
| Abuse/rate limits | `MC-SEC-006` | Limiter policy + deterministic `429` contract |
| Tool containment | `MC-SEC-007` | Sandbox policy schema + escape-resistance tests |
| Audit and forensics | `MC-SEC-008` | Security audit envelope + retention/redaction policy |
| Supply chain and vulnerability gate | `MC-SEC-009` | CI/nightly security checks + severity block policy |
| Incident runbooks and kill-switch ops | `MC-SEC-010` | Runbooks + drill evidence and SLOs |

## 14. Verification Evidence Checklist for Signoff

### Security Gate 0 Evidence
- Threat model approval artifact.
- JWT/authz/rate-limit/sandbox/audit controls enabled in environment.
- Security test suite reports from per-PR and nightly runs.
- Vulnerability summary showing unresolved critical/high count = 0.
- Kill-switch drill report with timing and blast-radius validation.

### API/Contract Evidence
- JWT claims contract document.
- Auth context propagation contract.
- AuthZ error taxonomy reference.
- `429` response envelope contract.
- Audit event envelope examples.
- Sandbox policy schema examples.
- Kill-switch precedence contract (`profile < provider < global`).

### Regression Evidence
- Existing session/run/approval/channel regression suites green with security controls enabled.
- Performance impact report for critical endpoints under enabled controls.

## Public APIs / Interfaces / Types to Add or Freeze
1. JWT claims contract (required claims, issuer/audience rules, clock skew policy).
2. Auth context propagation contract from edge gateway to carsinOS services (`principal_id`, `role_set`, `auth_method`, `token_id`/`jti`, `request_id`).
3. AuthZ policy error taxonomy (`AUTH_REQUIRED`, `AUTH_INVALID`, `AUTH_EXPIRED`, `AUTH_FORBIDDEN`, `AUTH_ROLE_MISMATCH`, `POLICY_DENY`).
4. Rate-limit response contract (`429` envelope with `error_code`, `scope`, `retry_after_seconds`, `request_id`).
5. Security audit event envelope (`event_id`, `request_id`, `principal`, `action`, `resource`, `decision`, `reason`, `timestamp_utc`, plus metadata fields).
6. Tool sandbox policy schema (`allowed_roots`, `allowed_binaries`, `network_policy`, risk mapping, timeout/output bounds, approval requirement).
7. Kill-switch control contract and precedence rules (`profile < provider < global`).

## Security Test Cases and Scenarios
1. JWT invalid signature, invalid issuer/audience, expired token, replayed token ID.
2. Role mismatch on approval resolution, auth profile mutation, and security admin endpoints.
3. Consumer OAuth high-risk mode enabled path logs warning and remains kill-switch controllable.
4. Secret rotation and revocation invalidate old credentials without service instability.
5. Header spoofing and untrusted-proxy behavior fail closed.
6. Rate-limit abuse on run/approval endpoints produces deterministic throttling and preserves gateway health.
7. Sandbox escape attempts via path traversal, shell expansion, symlink tricks, and disallowed binaries.
8. Audit trail completeness for create-run, tool-call, approval decision, authz deny, kill-switch toggle.
9. Kill-switch drills for profile/provider/global scopes with expected blast radius.
10. CI policy tests proving unresolved critical/high findings block release.
11. Nightly deep scans produce artifacts and diff against previous run.
12. Regression proof that existing session/run/approval/channel flows still pass with security controls active.

## Assumptions and Defaults
1. Internet-facing deployment is the primary target now.
2. Edge authentication is JWT-based behind an API gateway pattern.
3. Security controls are vendor-neutral; implementation may use any gateway that can enforce JWT and forward trusted auth context.
4. Consumer OAuth remains in production scope with high-risk controls, warnings, auditability, and kill-switch.
5. Security retention default is 90 days hot with archive policy.
6. Release policy blocks unresolved critical/high security findings.
7. Security checks are mandatory per PR; deeper scans run nightly.
8. Existing functional backlog remains valid but is gated by `MC-SEC` exit criteria for release readiness.

## Implementation Sequence (Decision-Complete)
1. Patch the ticket pack with `MC-SEC` phase, revised order, new gates, and sprint map.
2. Validate ticket dependency graph consistency and remove ambiguous wording.
3. Draft `SECURITY_HARDENING_PROGRAM.md` with the exact sections and control mappings in this document.
4. Add test matrix and evidence checklist.
5. Run document QA review for contradiction checks between packet and security doc.
6. Publish both docs as the single source of truth for the next implementation wave.
