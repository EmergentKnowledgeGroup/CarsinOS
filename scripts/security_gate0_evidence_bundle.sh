#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"
THREAT_MODEL_DOC="${REPO_ROOT}/docs/security/THREAT_MODEL_PACKAGE.md"
RUNBOOK_DOC="${REPO_ROOT}/docs/security/INCIDENT_RUNBOOKS.md"

mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/security-gate0-evidence-${TS}.log"
SUMMARY_FILE="${REPORT_DIR}/security-gate0-evidence-${TS}.json"
LATEST_FILE="${REPORT_DIR}/security-gate0-evidence-latest.json"

ALLOW_PENDING_APPROVALS="${ALLOW_PENDING_APPROVALS:-0}"
FINDINGS_FILE="${SECURITY_FINDINGS_FILE:-${REPORT_DIR}/finding-severity-latest.json}"
SECURITY_FINDINGS_CRITICAL="${SECURITY_FINDINGS_CRITICAL:-}"
SECURITY_FINDINGS_HIGH="${SECURITY_FINDINGS_HIGH:-}"

log() {
  printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${LOG_FILE}"
}

latest_matching() {
  local pattern="$1"
  shopt -s nullglob
  local files=("${REPORT_DIR}"/${pattern})
  shopt -u nullglob
  if [[ ${#files[@]} -eq 0 ]]; then
    return 0
  fi
  printf '%s\n' "${files[@]}" | sort | tail -n 1
}

json_status() {
  local file="$1"
  python3 - "$file" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(str(data.get("status", "")).strip())
PY
}

json_findings() {
  local file="$1"
  python3 - "$file" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)
critical = data.get("critical")
high = data.get("high")
source = data.get("source", "unknown")
if not isinstance(critical, int) or critical < 0:
    raise SystemExit("invalid critical count")
if not isinstance(high, int) or high < 0:
    raise SystemExit("invalid high count")
print(f"{critical},{high},{source}")
PY
}

is_threat_model_approved() {
  [[ -f "${THREAT_MODEL_DOC}" ]] || return 1
  grep -q '^\- Approval status: `approved`' "${THREAT_MODEL_DOC}" || return 1
  grep -q '^\- Decision: `approved`' "${THREAT_MODEL_DOC}" || return 1
  grep -q '^\- Threat model approver (`R4`): `TBD`' "${THREAT_MODEL_DOC}" && return 1
  grep -q '^\- Risk acceptance owner (`R4`): `TBD`' "${THREAT_MODEL_DOC}" && return 1
  return 0
}

is_runbook_owned() {
  [[ -f "${RUNBOOK_DOC}" ]] || return 1
  grep -q '^\- Incident primary (`R5`): `TBD`' "${RUNBOOK_DOC}" && return 1
  grep -q '^\- Incident backup (`R5`): `TBD`' "${RUNBOOK_DOC}" && return 1
  grep -q '^\- Threat model approver (`R4`): `TBD`' "${RUNBOOK_DOC}" && return 1
  grep -q '^\- Risk acceptance owner (`R4`): `TBD`' "${RUNBOOK_DOC}" && return 1
  return 0
}

log "Security Gate 0 evidence bundling start"
log "Repo root: ${REPO_ROOT}"
log "Report dir: ${REPORT_DIR}"

nightly_json="$(latest_matching 'nightly-deep-scan-*.json')"
killswitch_json="$(latest_matching 'killswitch-drill-*.json')"
secret_json="$(latest_matching 'secret-lifecycle-drill-*.json')"
pr_gate_log="$(latest_matching 'pr-gate-*.log')"

missing=0
if [[ -z "${nightly_json}" ]]; then
  log "FAIL missing nightly deep scan summary"
  missing=1
fi
if [[ -z "${killswitch_json}" ]]; then
  log "FAIL missing killswitch drill summary"
  missing=1
fi
if [[ -z "${secret_json}" ]]; then
  log "FAIL missing secret lifecycle drill summary"
  missing=1
fi
if [[ -z "${pr_gate_log}" ]]; then
  log "FAIL missing PR gate log"
  missing=1
fi
if [[ "${missing}" -ne 0 ]]; then
  log "FAIL missing required artifacts"
  exit 1
fi

nightly_status="$(json_status "${nightly_json}")"
killswitch_status="$(json_status "${killswitch_json}")"
secret_status="$(json_status "${secret_json}")"

critical_count=""
high_count=""
findings_source=""

if [[ -n "${SECURITY_FINDINGS_CRITICAL}" && -n "${SECURITY_FINDINGS_HIGH}" ]]; then
  critical_count="${SECURITY_FINDINGS_CRITICAL}"
  high_count="${SECURITY_FINDINGS_HIGH}"
  findings_source="env"
elif [[ -f "${FINDINGS_FILE}" ]]; then
  findings_csv="$(json_findings "${FINDINGS_FILE}")"
  critical_count="${findings_csv%%,*}"
  rest="${findings_csv#*,}"
  high_count="${rest%%,*}"
  findings_source="${rest#*,}"
else
  log "FAIL missing findings severity input; set SECURITY_FINDINGS_CRITICAL/HIGH or create ${FINDINGS_FILE}"
  exit 1
fi

threat_model_approved=false
if is_threat_model_approved; then
  threat_model_approved=true
fi

runbook_owned=false
if is_runbook_owned; then
  runbook_owned=true
fi

overall_status="green"
reasons=()

if [[ "${nightly_status}" != "green" ]]; then
  overall_status="red"
  reasons+=("nightly_status=${nightly_status}")
fi
if [[ "${killswitch_status}" != "green" ]]; then
  overall_status="red"
  reasons+=("killswitch_status=${killswitch_status}")
fi
if [[ "${secret_status}" != "green" ]]; then
  overall_status="red"
  reasons+=("secret_status=${secret_status}")
fi

if ! [[ "${critical_count}" =~ ^[0-9]+$ ]]; then
  overall_status="red"
  reasons+=("invalid_critical_count=${critical_count}")
fi
if ! [[ "${high_count}" =~ ^[0-9]+$ ]]; then
  overall_status="red"
  reasons+=("invalid_high_count=${high_count}")
fi

if [[ "${critical_count}" =~ ^[0-9]+$ ]] && [[ "${critical_count}" -gt 0 ]]; then
  overall_status="red"
  reasons+=("critical_findings=${critical_count}")
fi
if [[ "${high_count}" =~ ^[0-9]+$ ]] && [[ "${high_count}" -gt 0 ]]; then
  overall_status="red"
  reasons+=("high_findings=${high_count}")
fi

if [[ "${ALLOW_PENDING_APPROVALS}" != "1" ]]; then
  if [[ "${threat_model_approved}" != "true" ]]; then
    overall_status="red"
    reasons+=("threat_model_approval_pending")
  fi
  if [[ "${runbook_owned}" != "true" ]]; then
    overall_status="red"
    reasons+=("runbook_owner_assignment_pending")
  fi
fi

reason_json="[]"
if [[ ${#reasons[@]} -gt 0 ]]; then
  reason_json="["
  for idx in "${!reasons[@]}"; do
    item="${reasons[$idx]}"
    if [[ "${idx}" -gt 0 ]]; then
      reason_json+=","
    fi
    reason_json+="\"${item}\""
  done
  reason_json+="]"
fi

cat > "${SUMMARY_FILE}" <<JSON
{
  "timestamp_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "workflow": "security_gate0_evidence_bundle",
  "status": "${overall_status}",
  "allow_pending_approvals": ${ALLOW_PENDING_APPROVALS},
  "artifacts": {
    "nightly_summary": "${nightly_json}",
    "killswitch_summary": "${killswitch_json}",
    "secret_lifecycle_summary": "${secret_json}",
    "pr_gate_log": "${pr_gate_log}"
  },
  "findings": {
    "critical": ${critical_count},
    "high": ${high_count},
    "source": "${findings_source}"
  },
  "approvals": {
    "threat_model_approved": ${threat_model_approved},
    "runbook_owner_assigned": ${runbook_owned}
  },
  "reasons": ${reason_json}
}
JSON

cp "${SUMMARY_FILE}" "${LATEST_FILE}"

log "Summary: ${SUMMARY_FILE}"
log "Latest: ${LATEST_FILE}"

if [[ "${overall_status}" != "green" ]]; then
  log "FAIL Security Gate 0 evidence bundle is red"
  exit 1
fi

log "PASS Security Gate 0 evidence bundle is green"
