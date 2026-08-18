#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"
mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/secret-lifecycle-drill-${TS}.log"
SUMMARY_FILE="${REPORT_DIR}/secret-lifecycle-drill-${TS}.json"
DURATIONS_FILE="${REPORT_DIR}/secret-lifecycle-drill-${TS}.durations"

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
    printf '%s,%s\n' "${case_id}" "${duration}" >> "${DURATIONS_FILE}"
  else
    local ended
    ended="$(date +%s)"
    local duration
    duration=$((ended - started))
    log "FAIL  ${case_id} duration_sec=${duration}"
    exit 1
  fi
}

log "Secret lifecycle drill start"
log "Repo root: ${REPO_ROOT}"
log "Log file: ${LOG_FILE}"

cd "${REPO_ROOT}"
: > "${DURATIONS_FILE}"

run_case "api_rotate_secret" "rotate_auth_profile_secret_updates_secret_ref_and_deletes_previous_secret"
run_case "api_revoke_secret" "revoke_auth_profile_disables_profile_and_deletes_secret"
run_case "scheduled_rotate_secret" "scheduled_secret_rotation_job_rotates_ref_without_secret_leakage"
run_case "scheduled_revoke_secret" "scheduled_secret_revoke_job_disables_profile_and_deletes_secret"

average_duration="0"
if [[ -s "${DURATIONS_FILE}" ]]; then
  average_duration="$(awk -F',' '{sum+=$2; count+=1} END {if (count==0) print 0; else printf "%.2f", sum/count}' "${DURATIONS_FILE}")"
fi

cat > "${SUMMARY_FILE}" <<JSON
{
  "timestamp_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "workflow": "secret_lifecycle_drill",
  "status": "green",
  "average_case_duration_sec": ${average_duration},
  "log_file": "${LOG_FILE}",
  "durations_file": "${DURATIONS_FILE}"
}
JSON

log "Secret lifecycle drill complete"
log "Summary: ${SUMMARY_FILE}"
