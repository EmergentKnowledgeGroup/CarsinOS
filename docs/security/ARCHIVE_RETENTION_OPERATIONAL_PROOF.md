# Archive Retention Operational Proof

- Workflow ID: `SEC-RETENTION-PROOF-v1`
- Last updated (UTC): `2026-02-20`
- Scope: checklist `O7`

## Purpose
Prove that security audit records older than the hot window are archived and removable from the hot table, while in-window records are preserved.

## Command

```bash
scripts/security_archive_retention_proof.sh
```

## What it validates
1. Storage archive+delete lifecycle works (`security_audit_retention_archive_and_delete_work`).
2. 90-day hot-window boundary behavior works (`security_audit_retention_respects_ninety_day_hot_window`).
3. Gateway retention endpoint archives/prunes correctly (`security_audit_retention_run_archives_and_prunes_events`).
4. Gateway retention endpoint rejects invalid day ranges (`security_audit_retention_run_rejects_invalid_day_range`).

## Outputs
- Log:
  - `runtime/security/reports/archive-retention-proof-<timestamp>.log`
- JSON summary:
  - `runtime/security/reports/archive-retention-proof-<timestamp>.json`
- Rolling latest pointer:
  - `runtime/security/reports/archive-retention-proof-latest.json`

## Pass Criteria
- Summary `status=green`.
- All test cases in `tests[]` report `status=passed`.
- `hot_window_days` remains `90` unless policy revision is approved.

## Release Usage
- Attach `archive-retention-proof-latest.json` to release security evidence.
- If status is red, release progression is blocked until fixed and rerun.
