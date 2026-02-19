# Security Gate 0 Evidence Workflow

- Workflow ID: `SEC-GATE0-EVIDENCE-v1`
- Last updated (UTC): `2026-02-19`
- Scope: checklist `O3`

## 1. Purpose
Produce machine-readable release evidence for Security Gate 0 and fail closed if required evidence is missing or unresolved critical/high findings are non-zero.

## 2. Required Inputs
1. Latest nightly deep scan summary JSON (`nightly-deep-scan-*.json`).
2. Latest kill-switch drill summary JSON (`killswitch-drill-*.json`).
3. Latest secret lifecycle drill summary JSON (`secret-lifecycle-drill-*.json`).
4. Latest PR gate log (`pr-gate-*.log`).
5. Findings severity source provided by either:
   - environment variables `SECURITY_FINDINGS_CRITICAL` and `SECURITY_FINDINGS_HIGH`, or
   - JSON file (`runtime/security/reports/finding-severity-latest.json`) with:

```json
{
  "critical": 0,
  "high": 0,
  "source": "sast+deps+secrets"
}
```

## 3. Required Approval Inputs
- Threat model approval status from `docs/security/THREAT_MODEL_PACKAGE.md`.
- Incident runbook ownership from `docs/security/INCIDENT_RUNBOOKS.md`.

Default behavior is fail-closed when approvals are pending.

## 4. Execution

```bash
scripts/security_gate0_evidence_bundle.sh
```

Optional pre-release dry run while ownership/approval is still pending:

```bash
ALLOW_PENDING_APPROVALS=1 \
SECURITY_FINDINGS_CRITICAL=0 \
SECURITY_FINDINGS_HIGH=0 \
scripts/security_gate0_evidence_bundle.sh
```

## 5. Outputs
- Gate summary JSON:
  - `runtime/security/reports/security-gate0-evidence-<timestamp>.json`
- Gate log:
  - `runtime/security/reports/security-gate0-evidence-<timestamp>.log`
- Rolling pointer:
  - `runtime/security/reports/security-gate0-evidence-latest.json`

## 6. Pass Criteria
1. Drill summaries are present and each reports `status=green`.
2. PR gate log exists.
3. Critical findings count is `0`.
4. High findings count is `0`.
5. Threat model and runbook ownership approval checks pass (or explicit dry-run override).

## 7. Fail-Closed Conditions
- Missing required artifact.
- Drill status not green.
- Missing findings severity inputs.
- Findings `critical > 0` or `high > 0`.
- Pending approval/ownership when override is not enabled.

## 8. Release Usage
- Security Gate 0 must be green before release candidate tagging.
- Attach latest gate summary JSON to release evidence bundle.
- Any override use (`ALLOW_PENDING_APPROVALS=1`) is non-release and cannot be used for production signoff.
