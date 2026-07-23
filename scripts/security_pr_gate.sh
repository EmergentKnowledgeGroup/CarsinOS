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
run_step "clippy" cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets --features carsinos-gateway/execass-test-process-runtime -- -D warnings
run_step "tests-core" cargo test -p carsinos-tools -p carsinos-storage -p carsinos-gateway --features carsinos-gateway/execass-test-process-runtime -- --test-threads=1
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
  # Mission Control is a nested Cargo workspace with its own lockfile. Keep it
  # inside the release security boundary instead of auditing only the root CLI.
  run_step "cargo-audit-mission-control" cargo audit \
    --file "${REPO_ROOT}/apps/mission-control/src-tauri/Cargo.lock"
else
  if [[ "${require_cargo_audit}" == "1" ]]; then
    log "FAIL  cargo-audit missing. Install with: cargo install cargo-audit"
    exit 1
  fi
  log "WARN  cargo-audit missing; skipping because REQUIRE_CARGO_AUDIT=${require_cargo_audit}"
fi

log "Security PR gate complete"
log "Artifacts: ${LOG_FILE}"
