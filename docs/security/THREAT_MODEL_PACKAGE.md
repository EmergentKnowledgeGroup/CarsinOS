# carsinOS Threat Model Package

- Document ID: `TM-PACKAGE-v1`
- Scope: internet-facing gateway and adapter runtime
- Last updated (UTC): `2026-02-20`
- Approval status: `approved`
- Required approver (`R4`): `ProfessahX`

## 1. Purpose
This document is the Phase O threat model package for `MC-SEC-001`.
It defines trust boundaries, asset classes, attack paths, risk ratings, and mitigation ownership.

## 2. Trust Boundaries
1. Internet client -> API gateway edge
2. API gateway edge -> carsinOS gateway service
3. carsinOS gateway -> auth/profile/approval/session persistence
4. carsinOS gateway -> tool runtime (process/fs/network)
5. carsinOS gateway -> provider APIs
6. carsinOS gateway -> channel adapters (Telegram/Discord)
7. carsinOS gateway -> security reporting and audit retention backend

## 3. Asset Classification

| Asset | Class | Notes |
| --- | --- | --- |
| Auth/profile metadata | Sensitive | Includes auth mode, risk labels, kill-switch scope |
| Secret references | Restricted | Secret refs only; no plaintext key/token storage |
| Conversation/session payloads | Sensitive | May contain user or operational context |
| Tool input/output artifacts | Sensitive | May include file contents or command results |
| Security audit ledger | Restricted | Forensic chain must remain complete and tamper-evident |
| JWT principal and role claims | Sensitive | Used for authorization decisions |
| Channel routing identifiers | Sensitive | Guild/chat/channel mapping and policy controls |
| Benchmark and health telemetry | Internal | Operational performance data |

## 4. Gateway Surface Risk Classification

| Surface | Method(s) | Risk | Reason |
| --- | --- | --- | --- |
| `/` | GET | Low | Root service identity only |
| `/api/v1/health` | GET | Low | Liveness only |
| `/api/v1/status` | GET | Medium | Operational metadata exposure |
| `/api/v1/metrics` | GET | Medium | Capacity and behavior leakage risk |
| `/api/v1/providers/capabilities` | GET | Medium | Provider inventory + capability metadata |
| `/api/v1/extensions/plugins` | GET | Medium | Plugin footprint exposure |
| `/api/v1/extensions/skills` | GET | Medium | Skill index exposure |
| `/api/v1/extensions/skills/{skill_id}/state` | POST | High | Mutation of skill execution state |
| `/api/v1/agents` | GET/POST | High | Agent identity/model routing inventory and mutation |
| `/api/v1/agents/{agent_id}` | GET/POST | High | Agent metadata read/write mutation path |
| `/api/v1/boards` | GET | Medium | Board inventory exposure |
| `/api/v1/boards/{board_id}` | GET | High | Board card/script/asset metadata visibility |
| `/api/v1/boards/{board_id}/cards/create` | POST | High | Card mutation and execution planning pathway |
| `/api/v1/boards/{board_id}/cards/{card_id}/update` | POST | High | Card/script/owner mutation pathway |
| `/api/v1/boards/{board_id}/cards/{card_id}/move` | POST | Medium | Workflow ordering mutation |
| `/api/v1/boards/{board_id}/cards/{card_id}/assets/upload` | POST | High | Binary ingest, size/MIME abuse risk |
| `/api/v1/boards/{board_id}/cards/{card_id}/assets/{card_asset_id}` | GET | High | Asset access-control and data-exfiltration risk |
| `/api/v1/boards/{board_id}/cards/{card_id}/run` | POST | Critical | Direct run execution trigger from board workflow |
| `/api/v1/ws` | GET | High | Streaming channel with event leakage risk |
| `/api/v1/sessions` | GET/POST | High | Session lifecycle and model execution root |
| `/api/v1/sessions/{session_id}` | GET | High | Session content retrieval |
| `/api/v1/sessions/{session_id}/messages` | GET/POST | High | Message injection and content retrieval |
| `/api/v1/sessions/{session_id}/runs` | POST | Critical | Executes provider/tool workflows |
| `/api/v1/runs/{run_id}/resume` | POST | Critical | Resumes blocked high-risk execution |
| `/api/v1/memory/notes` | GET/POST | High | Long-term memory mutation risk |
| `/api/v1/memory/notes/{note_id}` | GET/POST | High | Memory read/write |
| `/api/v1/memory/search` | POST | High | Retrieval of sensitive memory artifacts |
| `/api/v1/auth/profiles` | GET/POST | Critical | Auth profile create/update pathways |
| `/api/v1/auth/profiles/{auth_profile_id}/state` | POST | Critical | Enable/disable high-risk auth paths |
| `/api/v1/auth/openai/oauth/start` | POST | High | OAuth initiation abuse risk |
| `/api/v1/auth/openai/oauth/finish` | POST | Critical | Token material ingress path |
| `/api/v1/auth/anthropic/setup-token/ingest` | POST | Critical | Token setup ingress path |
| `/api/v1/auth/agents/{agent_id}/providers/{provider}/profile-order` | GET/POST | High | Auth fallback/selection mutation |
| `/api/v1/config/channels` | GET/POST | Critical | Channel policy and routing config mutation |
| `/api/v1/channels/telegram/inbound` | POST | High | External inbound message ingestion |
| `/api/v1/channels/discord/inbound` | POST | High | External inbound message ingestion |
| `/api/v1/channels/approvals/resolve` | POST | Critical | Approval mutation by channel callback |
| `/api/v1/jobs` | GET | Medium | Scheduler inventory exposure |
| `/api/v1/jobs/status` | GET | Medium | Scheduler telemetry exposure |
| `/api/v1/jobs/add` | POST | High | New automation execution pathway |
| `/api/v1/jobs/{job_id}/update` | POST | High | Scheduler mutation |
| `/api/v1/jobs/{job_id}/remove` | POST | High | Job lifecycle mutation |
| `/api/v1/jobs/{job_id}/run` | POST | Critical | Immediate run execution trigger |
| `/api/v1/jobs/{job_id}/history` | GET | Medium | Execution history exposure |
| `/api/v1/mission-control/calendar/week` | GET | Medium | Aggregated schedule and run metadata exposure |
| `/api/v1/mission-control/focus` | GET | High | Aggregated approvals/failures/channel posture exposure |
| `/api/v1/approvals` | GET | High | Approval queue visibility |
| `/api/v1/approvals/request` | POST | Critical | Approval workflow mutation |
| `/api/v1/approvals/{approval_id}/resolve` | POST | Critical | Authorization decision endpoint |
| `/api/v1/security/audit` | GET | High | Forensic event visibility |
| `/api/v1/security/audit/retention/run` | POST | Critical | Retention/destructive action trigger |
| `/api/v1/security/auth-profiles/{auth_profile_id}/rotate-secret` | POST | Critical | Credential lifecycle mutation |
| `/api/v1/security/auth-profiles/{auth_profile_id}/revoke` | POST | Critical | Emergency auth profile disable/revoke |

## 5. STRIDE Threat Register

| ID | Boundary | Threat | STRIDE | Risk | Mitigations | Owner |
| --- | --- | --- | --- | --- | --- | --- |
| TM-001 | Internet -> edge | JWT spoofing with invalid signature/issuer | Spoofing | Critical | `MC-SEC-002` issuer/audience/signature validation | Security owner (`R4`) |
| TM-002 | Internet -> edge | Header spoof (`x-forwarded-for`, request-id) | Spoofing | High | `MC-SEC-005` trusted proxy enforcement | Security owner (`R4`) |
| TM-003 | Edge -> service | Role escalation via malformed claims | Elevation | Critical | `MC-SEC-003` deny-by-default role matrix | Security owner (`R4`) |
| TM-004 | Service -> providers | Unauthorized provider fallback path | Elevation | High | `A3`/`MC-PROV-002` risk-constrained fallback | Provider owner |
| TM-005 | Service -> tool runtime | Sandbox escape via path traversal/symlink | Elevation | Critical | `MC-SEC-007` allowlisted roots + deny tests | Tooling owner |
| TM-006 | Service -> tool runtime | Disallowed binary execution | Elevation | High | `MC-SEC-007` executable allowlist | Tooling owner |
| TM-007 | Service -> persistence | Secret leakage into logs/payloads | Information Disclosure | Critical | `MC-SEC-004` redaction + secret refs | Security owner (`R4`) |
| TM-008 | Service -> channels | Inbound channel spoof / unauthorized sender | Spoofing | High | allowlist + mention gating (`MC-CH-010/020`) | Channel owner |
| TM-009 | Approval flows | Replay/duplicate resolve action | Tampering | High | Idempotent approval resolution + audit chain | Approval owner |
| TM-010 | Scheduler | Run endpoint abuse / brute force | Denial of Service | High | `MC-SEC-006` per-IP/principal/endpoint rate limits | Platform owner |
| TM-011 | Audit trail | Tampering or deletion of forensic events | Repudiation | Critical | `MC-SEC-008` append-style ledger + retention policy | Security owner (`R4`) |
| TM-012 | OAuth/auth flows | Consumer OAuth misuse without visibility | Repudiation | High | high-risk warning + audit + kill-switch controls | Auth owner |
| TM-013 | Incident operations | Slow containment after key/token compromise | Denial of Service | Critical | `MC-SEC-010` kill-switch drills + runbooks | Incident owner (`R5`) |
| TM-014 | Config lifecycle | Hardcoded runtime values bypass policy controls | Tampering | High | `MC-CONF-001..005` wizard/config contract + CI guard | Platform owner |
| TM-015 | External dependencies | New critical CVE in dependencies | Denial of Service | High | `MC-SEC-009` per-PR + nightly supply chain gate | Security owner (`R4`) |

## 6. Control Mapping

| Control | Ticket(s) |
| --- | --- |
| Edge identity and authn | `MC-SEC-002` |
| Authorization and role policy | `MC-SEC-003` |
| Secret/key lifecycle | `MC-SEC-004` |
| Transport and proxy hardening | `MC-SEC-005` |
| Abuse/rate limiting | `MC-SEC-006` |
| Tool runtime containment | `MC-SEC-007` |
| Audit ledger and retention | `MC-SEC-008` |
| Dependency vulnerability gate | `MC-SEC-009` |
| Incident and kill-switch operations | `MC-SEC-010` |
| Setup wizard and hardcoded-value elimination | `MC-CONF-001` to `MC-CONF-005` |

## 7. Residual Risks and Required Approvals
- Residual risk RR-001: consumer OAuth remains high risk even with warning/audit/kill-switch.
- Residual risk RR-002: channel connectors depend on third-party API reliability.
- Residual risk RR-003: operator misconfiguration risk until setup wizard completeness gating is active.

Approval block:
- Threat model approver (`R4`): `ProfessahX`
- Risk acceptance owner (`R4`): `ProfessahX`
- Approved at (UTC): `2026-02-20T06:00:00Z`
- Decision: `approved-with-residual-risk`
