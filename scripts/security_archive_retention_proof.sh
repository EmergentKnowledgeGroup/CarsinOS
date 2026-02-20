#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"

mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/archive-retention-proof-${TS}.log"
SUMMARY_FILE="${REPORT_DIR}/archive-retention-proof-${TS}.json"
LATEST_FILE="${REPORT_DIR}/archive-retention-proof-latest.json"

log() {
  printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${LOG_FILE}"
}

run_case() {
  local case_id="$1"
  shift
  local cmd=("$@")
  local started_at finished_at duration status

  started_at="$(date +%s)"
  log "START ${case_id}: ${cmd[*]}"
  if "${cmd[@]}" >>"${LOG_FILE}" 2>&1; then
    status="passed"
  else
    status="failed"
  fi
  finished_at="$(date +%s)"
  duration="$((finished_at - started_at))"

  TEST_RESULTS_JSON+="{\"id\":\"${case_id}\",\"status\":\"${status}\",\"duration_seconds\":${duration}}"
  TEST_RESULTS_JSON+=","

  if [[ "${status}" == "passed" ]]; then
    log "PASS  ${case_id} (${duration}s)"
  else
    log "FAIL  ${case_id} (${duration}s)"
    OVERALL_STATUS="red"
  fi
}

OVERALL_STATUS="green"
TEST_RESULTS_JSON=""

log "Archive retention operational proof start"
log "Repo root: ${REPO_ROOT}"

run_case \
  "storage_retention_archive_delete" \
  cargo test -p carsinos-storage security_audit_retention_archive_and_delete_work -- --nocapture

run_case \
  "storage_retention_90d_boundary" \
  cargo test -p carsinos-storage security_audit_retention_respects_ninety_day_hot_window -- --nocapture

run_case \
  "gateway_retention_endpoint_archive" \
  cargo test -p carsinos-gateway security_audit_retention_run_archives_and_prunes_events -- --nocapture

run_case \
  "gateway_retention_endpoint_input_validation" \
  cargo test -p carsinos-gateway security_audit_retention_run_rejects_invalid_day_range -- --nocapture

if [[ -n "${TEST_RESULTS_JSON}" ]]; then
  TEST_RESULTS_JSON="[${TEST_RESULTS_JSON%,}]"
else
  TEST_RESULTS_JSON="[]"
fi

cat >"${SUMMARY_FILE}" <<JSON
{
  "timestamp_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "workflow": "archive_retention_operational_proof",
  "status": "${OVERALL_STATUS}",
  "hot_window_days": 90,
  "summary": {
    "log_file": "${LOG_FILE}",
    "report_file": "${SUMMARY_FILE}"
  },
  "tests": ${TEST_RESULTS_JSON}
}
JSON

cp "${SUMMARY_FILE}" "${LATEST_FILE}"

log "Summary: ${SUMMARY_FILE}"
log "Latest: ${LATEST_FILE}"

if [[ "${OVERALL_STATUS}" != "green" ]]; then
  log "FAIL archive retention operational proof"
  exit 1
fi

log "PASS archive retention operational proof"
