#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"
mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/killswitch-drill-${TS}.log"
SUMMARY_FILE="${REPORT_DIR}/killswitch-drill-${TS}.json"

log() {
  printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${LOG_FILE}"
}

run_case() {
  local case_id="$1"
  local test_filter="$2"

  log "START ${case_id} filter=${test_filter}"
  local started
  started="$(date +%s)"
  if cargo test -p carsinos-gateway "${test_filter}" -- --nocapture 2>&1 | tee -a "${LOG_FILE}"; then
    local ended
    ended="$(date +%s)"
    local duration
    duration=$((ended - started))
    log "PASS  ${case_id} duration_sec=${duration}"
    printf '%s,%s\n' "${case_id}" "${duration}" >> "${REPORT_DIR}/killswitch-drill-${TS}.durations"
  else
    local ended
    ended="$(date +%s)"
    local duration
    duration=$((ended - started))
    log "FAIL  ${case_id} duration_sec=${duration}"
    exit 1
  fi
}

log "Kill-switch drill start"
log "Repo root: ${REPO_ROOT}"
log "Log file: ${LOG_FILE}"

cd "${REPO_ROOT}"
: > "${REPORT_DIR}/killswitch-drill-${TS}.durations"

run_case "profile_scope_guard" "high_risk_auth_profile_requires_kill_switch"
run_case "provider_scope_guard" "provider_kill_switch_blocks_run_execution"
run_case "audit_and_policy_deny_chain" "role_mismatch_blocks_auth_profile_mutation_and_approval_resolution"

average_duration="0"
if [[ -s "${REPORT_DIR}/killswitch-drill-${TS}.durations" ]]; then
  average_duration="$(awk -F',' '{sum+=$2; count+=1} END {if (count==0) print 0; else printf "%.2f", sum/count}' "${REPORT_DIR}/killswitch-drill-${TS}.durations")"
fi

cat > "${SUMMARY_FILE}" <<JSON
{
  "timestamp_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "workflow": "killswitch_drill",
  "status": "green",
  "average_case_duration_sec": ${average_duration},
  "log_file": "${LOG_FILE}",
  "durations_file": "${REPORT_DIR}/killswitch-drill-${TS}.durations"
}
JSON

log "Kill-switch drill complete"
log "Summary: ${SUMMARY_FILE}"
