#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPORT_DIR="${REPO_ROOT}/runtime/security/reports"
mkdir -p "${REPORT_DIR}"

TS="$(date -u +"%Y%m%dT%H%M%SZ")"
LOG_FILE="${REPORT_DIR}/pr-gate-${TS}.log"

require_cargo_audit="${REQUIRE_CARGO_AUDIT:-1}"

log() {
  printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${LOG_FILE}"
}

run_step() {
  local label="$1"
  shift
  log "START ${label}: $*"
  if "$@" 2>&1 | tee -a "${LOG_FILE}"; then
    log "PASS  ${label}"
  else
    log "FAIL  ${label}"
    exit 1
  fi
}

has_cargo_audit() {
  cargo audit -V >/dev/null 2>&1
}

log "Security PR gate start"
log "Repo root: ${REPO_ROOT}"
log "Log file: ${LOG_FILE}"

cd "${REPO_ROOT}"

run_step "fmt" cargo fmt --all --check
run_step "clippy" cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings
run_step "tests-core" cargo test -p carsinos-tools -p carsinos-storage -p carsinos-gateway -- --test-threads=1
run_step "tests-workspace" cargo test --workspace \
  --exclude carsinos-gateway \
  --exclude carsinos-storage \
  --exclude carsinos-tools \
  -- --test-threads=1
run_step "hardcoded-value-guard" python3 "${SCRIPT_DIR}/security_hardcoded_value_guard.py" --repo-root "${REPO_ROOT}"

if has_cargo_audit; then
  # quick-xml is reachable only through the wayland-scanner build-time proc
  # macro, which parses version-pinned dependency XML rather than runtime input.
  run_step "cargo-audit" cargo audit \
    --ignore RUSTSEC-2026-0194 \
    --ignore RUSTSEC-2026-0195
else
  if [[ "${require_cargo_audit}" == "1" ]]; then
    log "FAIL  cargo-audit missing. Install with: cargo install cargo-audit"
    exit 1
  fi
  log "WARN  cargo-audit missing; skipping because REQUIRE_CARGO_AUDIT=${require_cargo_audit}"
fi

log "Security PR gate complete"
log "Artifacts: ${LOG_FILE}"
