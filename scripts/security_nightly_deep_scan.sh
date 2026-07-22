#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" == "Linux" && -z "${XDG_RUNTIME_DIR:-}" ]]; then
  export XDG_RUNTIME_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/carsinos-runtime-${UID}"
  install -d -m 700 "${XDG_RUNTIME_DIR}"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"
mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/nightly-deep-scan-${TS}.log"
SUMMARY_FILE="${REPORT_DIR}/nightly-deep-scan-${TS}.json"

require_cargo_audit="${REQUIRE_CARGO_AUDIT:-1}"

log() {
  printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${LOG_FILE}"
}

run_step() {
  local label="$1"
  shift
  log "START ${label}: $*"
  local started
  started="$(date +%s)"
  if "$@" 2>&1 | tee -a "${LOG_FILE}"; then
    local ended
    ended="$(date +%s)"
    log "PASS  ${label} duration_sec=$((ended - started))"
  else
    local ended
    ended="$(date +%s)"
    log "FAIL  ${label} duration_sec=$((ended - started))"
    exit 1
  fi
}

log "Nightly deep scan start"
log "Repo root: ${REPO_ROOT}"
log "Log file: ${LOG_FILE}"

cd "${REPO_ROOT}"

run_step "security-pr-gate" env REQUIRE_CARGO_AUDIT="${require_cargo_audit}" "${SCRIPT_DIR}/security_pr_gate.sh"
run_step "benchmarks" cargo test -p carsinos-gateway --test benchmark_process -- --nocapture
run_step "gateway-e2e" cargo test -p carsinos-gateway --features execass-test-process-runtime --test e2e_process
run_step "secret-lifecycle-drill" "${SCRIPT_DIR}/security_secret_lifecycle_drill.sh"
run_step "killswitch-drill" "${SCRIPT_DIR}/security_killswitch_drill.sh"

if cargo audit -V >/dev/null 2>&1; then
  # These two quick-xml advisories are reachable only through Wayland's build-time
  # protocol scanner. SECURITY.md records the threat analysis and removal policy.
  # Keep the nightly JSON scan aligned with the PR gate so a documented,
  # non-runtime exception does not make every nightly appear compromised.
  run_step "cargo-audit-json" sh -c "cargo audit \
    --ignore RUSTSEC-2026-0194 \
    --ignore RUSTSEC-2026-0195 \
    --json > '${REPORT_DIR}/cargo-audit-${TS}.json'"
  run_step "cargo-audit-mission-control-json" sh -c "cargo audit \
    --file '${REPO_ROOT}/apps/mission-control/src-tauri/Cargo.lock' \
    --json > '${REPORT_DIR}/cargo-audit-mission-control-${TS}.json'"
fi

cat > "${SUMMARY_FILE}" <<JSON
{
  "timestamp_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "workflow": "nightly_deep_scan",
  "status": "green",
  "log_file": "${LOG_FILE}",
  "report_dir": "${REPORT_DIR}",
  "artifacts": [
    "${LOG_FILE}",
    "${SUMMARY_FILE}"
  ]
}
JSON

log "Nightly deep scan complete"
log "Summary: ${SUMMARY_FILE}"
