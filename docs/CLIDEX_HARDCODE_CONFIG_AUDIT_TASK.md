# CLIdex Task: Hardcoded Runtime-Value Audit (MC-CONF-005)

## Objective
Find and classify every runtime/deployment-specific hardcoded value that should be configuration-driven in carsinOS, then produce an actionable conversion report.

## Why This Matters
carsinOS must support multiple operators/environments without source edits. Runtime IDs/tokens/domains/policies must be set through config/wizard paths, not code constants.

## Scope
Audit all runtime paths in:
- `crates/`
- `scripts/`
- `.github/workflows/`
- root config/bootstrap files (`Cargo.toml`, `.env` templates, startup scripts)

Do not skip files because they "look test-like" until you classify them.

## Exclusions (Allowed Constants)
These are allowed if clearly documented:
- Protocol/schema constants (error code strings, enum names, route templates)
- Pure compile-time behavior constants not tied to deployment identity
- Test fixtures under test-only modules (must remain test-scoped)

If in doubt, classify as `needs-review`.

## High-Risk Hardcoded Classes
Treat as high priority if found outside test fixtures:
- JWT issuer/audience defaults tied to a real environment
- Trusted proxy/header allowlists tied to one deployment
- Channel IDs, guild IDs, chat IDs, bot app IDs
- Domain names, webhook URLs, callback URLs
- OAuth mode defaults that bypass operator decisioning
- Archive bucket names, retention destinations, owner identity mappings
- Tokens/keys/secrets (any plaintext credential)

## Audit Method (Use Fast Grep)
Run targeted scans and aggregate findings:

```bash
rg -n --hidden --glob '!.git' --glob '!target' \
  -e 'issuer|audience|trusted[_-]?proxy|x-forwarded|webhook|callback' \
  -e 'telegram|discord|guild|chat_id|channel_id|bot_token|application_id' \
  -e 'oauth|consumer|retention|archive|bucket|kms|owner|on_call|kill[-_]?switch' \
  crates scripts .github

rg -n --hidden --glob '!.git' --glob '!target' \
  -e 'sk-[A-Za-z0-9]+' -e 'xoxb-' -e 'ghp_' -e 'AIza' -e 'BEGIN (RSA|EC|OPENSSH) PRIVATE KEY' \
  .
```

Then manually inspect each match for runtime relevance.

## Classification Schema
For each finding, capture:
- `id`: `HCV-###`
- `file`
- `line`
- `current_value_summary`
- `risk`: `critical|high|medium|low|needs-review`
- `reason`
- `proposed_config_scope`: `global|provider|auth_profile|channel|security`
- `proposed_config_key`
- `migration_note`

## Deliverables
1. Create report:
   - `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`
2. Include summary counts by risk.
3. Include top-10 remediation sequence.
4. Include an explicit "allowlist candidates" section with owner + expiry suggestion.

## Acceptance Criteria
- All high/critical runtime hardcoded findings are identified.
- Every finding maps to a proposed config key/scope.
- Report is sufficient for implementation without re-discovery.
- No plaintext secrets remain unflagged.

## Coordination Notes
- This task directly supports checklist items `O9` and `P5` in `CHECKLIST.md`.
- If you submit patches, keep them separated from the report PR to simplify review.
