# HARDCODED_RUNTIME_VALUES_AUDIT.md

## Objective
Identify runtime/deployment hardcoded values that should be operator-configurable, then map remediation into execution tickets.

## Scope + Method
Scanned:
- `crates/`
- `scripts/`
- `.github/workflows/`
- Runtime docs and config artifacts

Commands used:

```bash
rg -n --hidden --glob '!.git' --glob '!target' \
  -e 'issuer|audience|trusted[_-]?proxy|x-forwarded|webhook|callback' \
  -e 'telegram|discord|guild|chat_id|channel_id|bot_token|application_id' \
  -e 'oauth|consumer|retention|archive|bucket|kms|owner|on_call|kill[-_]?switch' \
  crates scripts .github docs CHECKLIST.md SECURITY_HARDENING_PROGRAM.md APPDEX_IMPLEMENTATION_TICKET_PACK.md

rg -n --hidden --glob '!.git' --glob '!target' \
  -e 'sk-[A-Za-z0-9]+' -e 'xoxb-' -e 'ghp_' -e 'AIza' -e 'BEGIN (RSA|EC|OPENSSH) PRIVATE KEY' \
  crates scripts .github docs

python3 scripts/security_hardcoded_value_guard.py --repo-root .
```

## Findings Matrix

| id | file:line | current_value_summary | risk | reason | proposed_config_scope | proposed_config_key | migration_note |
|---|---|---|---|---|---|---|---|
| HCV-001 | `crates/carsinos-gateway/src/main.rs:6747` | Numquam integration base URL defaults to `http://127.0.0.1:7340` | high | Can silently point integration to wrong endpoint in non-local deploys. | global | `global.numquam.base_url` | Add to runtime config/wizard; env var becomes fallback-only compatibility layer. |
| HCV-002 | `crates/carsinos-gateway/src/main.rs:6771` | Numquam principal ID defaults to `carsinos_gateway` | medium | Identity metadata should be operator-owned for audit consistency. | security | `security.integration_principal_id` | Populate from wizard; deny empty in internet-facing mode. |
| HCV-003 | `crates/carsinos-gateway/src/main.rs:6775` | Numquam principal display name defaults to `carsinOS Gateway` | low | Cosmetic but still identity metadata used in logs/audit. | security | `security.integration_principal_name` | Wizard text field with validation length + character set. |
| HCV-004 | `crates/carsinos-gateway/src/main.rs:717` | OpenAI OAuth redirect URI defaults to `http://127.0.0.1:1455/auth/callback` | medium | Wrong redirect in hosted deployments causes auth failures/misrouting. | global | `global.oauth.openai.redirect_uri` | Prefer runtime config; keep request/env override precedence. |
| HCV-005 | `crates/carsinos-core/src/lib.rs:29` | Gateway bind defaults to `127.0.0.1:18789` | medium | Network exposure behavior should be explicit in operator setup. | global | `global.gateway.bind` | Wizard network step writes bind + edge TLS/public bind policy pair. |
| HCV-006 | `crates/carsinos-gui/src/main.rs:244` | GUI gateway URL defaults to `http://127.0.0.1:18789` | medium | GUI may target wrong gateway when operators run remote control-plane. | global | `global.gui.gateway_base_url` | Read from runtime config API; keep env var as override. |
| HCV-007 | `crates/carsinos-providers/src/lib.rs:344` | vLLM base URL defaults to `http://127.0.0.1:8000` | medium | Provider endpoint is deployment-specific and should not require source edits. | provider | `providers.vllm.default_api_base_url` | Source from provider profile/runtime config before fallback. |
| HCV-008 | `crates/carsinos-providers/src/lib.rs:377` | Ollama base URL defaults to `http://127.0.0.1:11434` | medium | Same operational coupling as HCV-007. | provider | `providers.ollama.default_api_base_url` | Source from provider profile/runtime config before fallback. |
| HCV-009 | `crates/carsinos-tools/src/lib.rs:13` | Tool network allowlist includes hardcoded `api.duckduckgo.com` | high | External egress host policy should be operator-controlled for compliance. | security | `security.tool_network_allowlist` | Move defaults to config contract; enforce non-empty explicit allowlist in internet mode. |
| HCV-010 | `crates/carsinos-gateway/src/main.rs:8195` | Channel auto-run default model is hardcoded (`mock`, `mock-echo-v1`) | low | Not deployment identity but causes implicit runtime behavior drift. | channel | `channels.<provider>.default_model_provider` + `channels.<provider>.default_model_id` | Keep dev fallback but require wizard explicit set before release-ready state. |
| HCV-011 | `crates/carsinos-gateway/src/main.rs:10489` | Allowed channel providers fallback is hardcoded (`telegram,discord`) | low | Feature gating should track enabled channel config, not source literal. | global | `global.channel_tool.allowed_providers` | Derive from runtime channel enablement by default; config override optional. |

## Risk Summary
- high: 2
- medium: 6
- low: 3
- critical: 0
- needs-review: 0

## Top-10 Remediation Sequence
1. HCV-001 `global.numquam.base_url`
2. HCV-009 `security.tool_network_allowlist`
3. HCV-004 `global.oauth.openai.redirect_uri`
4. HCV-005 `global.gateway.bind`
5. HCV-006 `global.gui.gateway_base_url`
6. HCV-007 `providers.vllm.default_api_base_url`
7. HCV-008 `providers.ollama.default_api_base_url`
8. HCV-002 `security.integration_principal_id`
9. HCV-010 `channels.*.default_model_*`
10. HCV-011 `global.channel_tool.allowed_providers`

## Allowlist Candidates (Intentional Constants)
These are candidates to remain source constants with explicit allowlist metadata:

| constant | rationale | owner | expiry_suggestion |
|---|---|---|---|
| `OAUTH_OPENAI_DEFAULT_AUTHORIZE_URL` (`crates/carsinos-gateway/src/main.rs:719`) | Canonical provider auth endpoint default, request/env override already exists. | provider-platform | 2026-12-31 |
| `OAUTH_OPENAI_DEFAULT_TOKEN_URL` (`crates/carsinos-gateway/src/main.rs:720`) | Canonical provider token endpoint default, request/env override already exists. | provider-platform | 2026-12-31 |
| `OPENAI_DEFAULT_API_BASE` (`crates/carsinos-gateway/src/main.rs:721`) | Canonical OpenAI API base fallback; configurable elsewhere. | provider-platform | 2026-12-31 |
| `ANTHROPIC_DEFAULT_API_BASE` (`crates/carsinos-gateway/src/main.rs:722`) | Canonical Anthropic API base fallback; configurable at ingest. | provider-platform | 2026-12-31 |
| `DEFAULT_TOOL_ALLOWED_BINARIES` (`crates/carsinos-tools/src/lib.rs:14`) | Safe baseline binary list for sandbox start state; policy can override by env/config. | security-platform | 2026-12-31 |

## Ticket Triage (O10)
Implementation tickets grouped by scope and milestone:

| ticket_id | scope | covers | owner | target_milestone |
|---|---|---|---|---|
| MC-CONF-006 | global | HCV-001, HCV-004, HCV-005, HCV-006, HCV-011 | platform-gateway | Sprint A+1 |
| MC-CONF-007 | provider | HCV-007, HCV-008 | provider-runtime | Sprint A+1 |
| MC-CONF-008 | security | HCV-002, HCV-003, HCV-009 | security-platform | Sprint A+1 |
| MC-CONF-009 | channel | HCV-010 | channels-runtime | Sprint A+2 |

## Acceptance for This Audit Deliverable
- All identified high/medium operational hardcoded values map to a config key.
- Findings include concrete file/line provenance.
- Remediation is sequenced and ticketized by scope.
- Guard script baseline remains green (`runtime/security/reports/hardcoded-value-guard-*.json`).
