# carsinOS Incident Runbooks

- Document ID: `IR-PACKAGE-v1`
- Scope: `MC-SEC-010`
- Last updated (UTC): `2026-02-20`
- Ownership status: `assigned`
- Primary owner (`R5`): `ProfessahX`
- Backup owner (`R5`): `ProfessahX`

## 1. Severity Model
- `SEV-1`: active compromise/exfiltration or broad unauthorized execution risk.
- `SEV-2`: confirmed high-risk misuse with bounded blast radius.
- `SEV-3`: suspicious or failed attack with no confirmed compromise.

## 2. Kill-Switch Precedence
1. `profile` scope kill-switch (narrowest blast radius)
2. `provider` scope kill-switch
3. `global` kill-switch (last resort)

Rule: escalate from narrowest to broadest only if containment fails.

## 3. Common Response Checklist
1. Create incident ID and set severity.
2. Freeze mutable evidence sources (audit logs, gateway logs, related run/session IDs).
3. Apply smallest kill-switch scope that contains risk.
4. Verify containment by checking denied actions and halted risky paths.
5. Capture detection timestamp and containment completion timestamp.
6. Notify owners and stakeholders with current blast radius.
7. Start root-cause and remediation plan.

## 4. Runbook A: Token/Auth Compromise
### Trigger Conditions
- Unexpected successful auth events from unknown principals.
- Replay token (`jti`) anomalies.
- Auth profile usage from unapproved source.

### Immediate Actions (0-15 min)
1. Activate profile kill-switch for affected auth profile(s).
2. Revoke affected credentials/tokens.
3. Force refresh of role/claim allowlists and deny replayed `jti` values.

### Containment Verification (15-45 min)
1. Confirm blocked auth attempts return stable auth denial codes.
2. Confirm no new runs execute with revoked profile.
3. Confirm security audit chain contains actor/action/result sequence.

### Recovery
1. Rotate credentials through approved secret lifecycle flow.
2. Re-enable profile only after explicit owner approval.
3. Document root cause and compensating controls.

## 5. Runbook B: Secret/Key Leak
### Trigger Conditions
- Secret material appears in logs, screenshots, or commit diff.
- External alert flags leaked token/key.

### Immediate Actions (0-15 min)
1. Revoke leaked secret immediately.
2. Disable affected profile/provider via kill-switch until rotated.
3. Stop jobs/runs relying on leaked credential.

### Containment Verification (15-45 min)
1. Confirm old credential is rejected.
2. Confirm replacement credential works only through secret-ref path.
3. Confirm redaction controls prevent repeated leakage.

### Recovery
1. Rotate all sibling credentials in same trust domain.
2. Backfill audit report with exposure window and usage events.
3. Publish follow-up hardening action items.

## 6. Runbook C: Provider Abuse / Spend Anomaly
### Trigger Conditions
- Sudden run-volume spike or anomalous provider calls.
- Unapproved profile selected at runtime.

### Immediate Actions (0-15 min)
1. Enable provider kill-switch for affected provider.
2. Disable suspect auth profiles and scheduler jobs.
3. Tighten rate limits on run and approval endpoints.

### Containment Verification (15-45 min)
1. Confirm provider calls stop for disabled scope.
2. Confirm scheduler no longer dispatches suspect jobs.
3. Confirm audit trail continuity for every kill-switch action.

### Recovery
1. Re-enable minimal safe profile set.
2. Add additional policy constraints and alert thresholds.
3. Capture billing/usage deltas for postmortem.

## 7. Runbook D: Tool Runtime Abuse or Sandbox Violation
### Trigger Conditions
- Disallowed binary or path traversal attempts.
- Unexpected network egress from restricted tool path.

### Immediate Actions (0-15 min)
1. Disable offending tool capability or profile scope.
2. Move to strict deny-all network policy for tool runtime.
3. Require approvals for all high-risk tool invocations.

### Containment Verification (15-45 min)
1. Confirm policy denies are enforced and audited.
2. Confirm no successful high-risk tool execution without approval.
3. Confirm filesystem boundaries remain intact.

### Recovery
1. Patch sandbox policy allowlists/denylists.
2. Add test reproducer for observed abuse pattern.
3. Re-enable tool path only after successful drill replay.

## 8. Runbook E: Suspected Data Exfiltration
### Trigger Conditions
- Unexpected outbound payload volume.
- Sensitive content appears in unauthorized destination.

### Immediate Actions (0-15 min)
1. Apply global kill-switch if blast radius is unknown.
2. Disable channel/provider egress paths with highest risk.
3. Snapshot and preserve all related logs and audit artifacts.

### Containment Verification (15-45 min)
1. Confirm outbound mutation/event volume drops to expected baseline.
2. Confirm unauthorized destinations are no longer receiving payloads.
3. Confirm evidence chain completeness (`request_id`, `principal`, `action`, `resource`).

### Recovery
1. Re-enable services in controlled stages (profile -> provider -> global).
2. Conduct full blast-radius review and legal/compliance routing if needed.
3. Create permanent prevention actions and retest drills.

## 9. Evidence Requirements Per Incident
- Incident ID
- Detection timestamp
- Containment completion timestamp
- Kill-switch scope and rationale
- Affected principal/profile/provider IDs
- Linked audit events and logs
- Final blast radius statement
- Recovery signoff owner

## 10. Drill and Readiness Cadence
- Live drill cadence: quarterly minimum.
- Triggered drill: after major auth/security/runtime changes.
- Mandatory scripts:
  - `scripts/security_killswitch_drill.sh`
  - `scripts/security_secret_lifecycle_drill.sh`
- Success criteria:
  - All drill scenarios pass.
  - Detection and containment times are captured.
  - No missing audit chain fields.

## 11. Ownership Assignment Block (Required)
- Threat model approver (`R4`): `ProfessahX`
- Risk acceptance owner (`R4`): `ProfessahX`
- Incident primary (`R5`): `ProfessahX`
- Incident backup (`R5`): `ProfessahX`
- Effective date: `2026-02-20`
