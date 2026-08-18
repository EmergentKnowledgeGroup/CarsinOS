#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

MODE="web"
GATEWAY_HOST="127.0.0.1"
GATEWAY_PORT="${CARSINOS_LAUNCH_GATEWAY_PORT:-18789}"
UI_PORT="${CARSINOS_LAUNCH_UI_PORT:-1420}"
STATE_DIR="${CARSINOS_STATE_DIR:-${REPO_ROOT}/runtime/oneclick-state}"
TOKEN="${CARSINOS_GATEWAY_TOKEN:-}"
VALIDATE_ONLY=0
MC_APP_DIR="${REPO_ROOT}/apps/mission-control"
MC_ENV_FILE="${MC_APP_DIR}/.env.development.local"
MC_ENV_BACKUP=""
MNO_LANES_DIR="${STATE_DIR}/mno-lanes"
PID_DIR="${STATE_DIR}/pids"
LAUNCHER_PID_FILE="${PID_DIR}/oneclick-launcher.pid"
GATEWAY_PID_FILE="${PID_DIR}/gateway.pid"
UI_PID_FILE="${PID_DIR}/mission-control-ui.pid"
DEPLOYMENT_DIR="${STATE_DIR}/deployment"
TRUST_LOCK_FILE="${DEPLOYMENT_DIR}/trust_contract.lock.json"

GATEWAY_PID=""
UI_PID=""
GATEWAY_START_ATTEMPTED_RECOVERY=0

usage() {
  cat <<'EOF'
Usage: scripts/one_click_launch.sh [options]

Options:
  --web                     Launch Mission Control in web mode (default).
  --tauri                   Launch Mission Control using tauri dev (requires port 1420 free).
  --gateway-port <port>     Preferred gateway port (default: 18789; falls forward if busy).
  --ui-port <port>          Preferred UI port in web mode (default: 1420; falls forward if busy).
  --gateway-host <host>     Gateway host bind (default: 127.0.0.1).
  --token <value>           Use explicit gateway token.
  --validate-only            Validate the development-state fence without starting processes.
  --help                    Show this help.
EOF
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "Missing required command: ${name}" >&2
    exit 1
  fi
}

state_root_fence_identity() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).expanduser().resolve(strict=False))
PY
}

production_state_root() {
  case "$(uname -s)" in
    Darwin) printf '%s\n' "${HOME}/Library/Application Support/io.carsinos.missioncontrol/state" ;;
    Linux) printf '%s\n' "${XDG_DATA_HOME:-${HOME}/.local/share}/io.carsinos.missioncontrol/state" ;;
    *)
      echo "Unsupported platform for the Mission Control production state-root fence: $(uname -s)" >&2
      return 1
      ;;
  esac
}

assert_development_state_root() {
  local candidate candidate_identity production production_identity production_raw
  candidate="$(state_root_fence_identity "${STATE_DIR}")"
  if ! production_raw="$(production_state_root)"; then
    return 1
  fi
  production="$(state_root_fence_identity "${production_raw}")"
  candidate_identity="${candidate}"
  production_identity="${production}"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    candidate_identity="$(printf '%s' "${candidate}" | tr '[:upper:]' '[:lower:]')"
    production_identity="$(printf '%s' "${production}" | tr '[:upper:]' '[:lower:]')"
  fi
  if [[ "${candidate_identity}" == "${production_identity}" ]]; then
    echo "Legacy one-click launch is development-only and refuses the canonical Mission Control production state root: ${production}. Choose a separate development CARSINOS_STATE_DIR." >&2
    return 1
  fi
  STATE_DIR="${candidate}"
}

is_port_in_use() {
  local port="$1"
  lsof -nP -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
}

pid_command() {
  local pid="$1"
  ps -p "${pid}" -o command= 2>/dev/null || true
}

pid_cwd() {
  local pid="$1"
  lsof -a -p "${pid}" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1
}

pid_belongs_to_repo() {
  local pid="$1"
  local cmd cwd
  cmd="$(pid_command "${pid}")"
  cwd="$(pid_cwd "${pid}")"
  [[ "${cmd}" == *"${REPO_ROOT}"* ]] || [[ "${cwd}" == "${REPO_ROOT}"* ]]
}

wait_for_pid_exit() {
  local pid="$1"
  for _ in $(seq 1 30); do
    if ! kill -0 "${pid}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

collect_descendant_pids() {
  local pid="$1"
  local child
  local children

  children="$(pgrep -P "${pid}" 2>/dev/null || true)"
  for child in ${children}; do
    collect_descendant_pids "${child}"
    printf '%s\n' "${child}"
  done
}

stop_pid_tree_if_running() {
  local pid="$1"
  local label="$2"
  local descendant
  local descendants=""
  if ! kill -0 "${pid}" >/dev/null 2>&1; then
    return 0
  fi

  while IFS= read -r descendant; do
    [[ -n "${descendant}" ]] || continue
    descendants+="${descendant}"$'\n'
  done < <(collect_descendant_pids "${pid}")

  echo "Reclaiming ${label} (pid ${pid})."
  while IFS= read -r descendant; do
    [[ -n "${descendant}" ]] || continue
    kill "${descendant}" >/dev/null 2>&1 || true
  done <<< "${descendants}"
  kill "${pid}" >/dev/null 2>&1 || true
  if wait_for_pid_exit "${pid}"; then
    return 0
  fi

  echo "Force stopping ${label} (pid ${pid})."
  while IFS= read -r descendant; do
    [[ -n "${descendant}" ]] || continue
    kill -9 "${descendant}" >/dev/null 2>&1 || true
  done <<< "${descendants}"
  kill -9 "${pid}" >/dev/null 2>&1 || true
  wait_for_pid_exit "${pid}" || true
}

write_pid_file() {
  local file="$1"
  local pid="$2"
  mkdir -p "${PID_DIR}"
  printf '%s\n' "${pid}" > "${file}"
}

clear_pid_file() {
  local file="$1"
  rm -f "${file}" >/dev/null 2>&1 || true
}

stop_pid_file_process() {
  local file="$1"
  local label="$2"
  local pid

  if [[ ! -f "${file}" ]]; then
    return 0
  fi

  pid="$(tr -cd '0-9' < "${file}")"
  clear_pid_file "${file}"
  if [[ -z "${pid}" ]] || [[ "${pid}" == "$$" ]]; then
    return 0
  fi
  if kill -0 "${pid}" >/dev/null 2>&1 && pid_belongs_to_repo "${pid}"; then
    stop_pid_tree_if_running "${pid}" "${label}"
  fi
}

reclaim_repo_listener_on_port() {
  local port="$1"
  local label="$2"
  local pids pid
  pids="$(lsof -t -nP -iTCP:"${port}" -sTCP:LISTEN 2>/dev/null || true)"
  for pid in ${pids}; do
    if [[ "${pid}" == "$$" ]]; then
      continue
    fi
    if pid_belongs_to_repo "${pid}"; then
      stop_pid_tree_if_running "${pid}" "${label} listener on port ${port}"
    fi
  done
}

reclaim_repo_runtime_processes() {
  local pid line
  while IFS= read -r line; do
    pid="${line%% *}"
    [[ -n "${pid}" ]] || continue
    [[ "${pid}" =~ ^[0-9]+$ ]] || continue
    if [[ "${pid}" == "$$" ]] || [[ "${pid}" == "$PPID" ]]; then
      continue
    fi
    if ! pid_belongs_to_repo "${pid}"; then
      continue
    fi
    case "${line}" in
      *"/scripts/one_click_launch.sh"*|*"/debug/carsinos-gateway"*|*"cargo run -p carsinos-gateway"*|*"/debug/carsinos-mission-control"*|*"npm run tauri:dev"*|*"vite --host --port"*|*"npm run dev -- --host 127.0.0.1 --port"*|*"run_live_runtime.py"*|*"run_mcp_server.py"*)
        stop_pid_tree_if_running "${pid}" "repo-owned one-click runtime"
        ;;
    esac
  done < <(pgrep -af "${REPO_ROOT}" 2>/dev/null || true)
}

cleanup_stale_mission_control_vite() {
  local cleanup_port="${UI_PORT_SELECTED:-${UI_PORT}}"
  reclaim_repo_listener_on_port "${cleanup_port}" "Mission Control UI"
  if [[ "${cleanup_port}" != "1420" ]]; then
    reclaim_repo_listener_on_port 1420 "Mission Control UI"
  fi
}

cleanup_stale_gateway_listener() {
  local cleanup_port="${GATEWAY_PORT_SELECTED:-${GATEWAY_PORT}}"
  reclaim_repo_listener_on_port "${cleanup_port}" "gateway"
}

cleanup_oneclick_runtime_residue() {
  clear_pid_file "${LAUNCHER_PID_FILE}"
  clear_pid_file "${UI_PID_FILE}"
  clear_pid_file "${GATEWAY_PID_FILE}"
  rm -f "${TRUST_LOCK_FILE}" >/dev/null 2>&1 || true
  rmdir "${DEPLOYMENT_DIR}" >/dev/null 2>&1 || true
}

reclaim_previous_oneclick_runtime() {
  mkdir -p "${STATE_DIR}" "${PID_DIR}"
  stop_pid_file_process "${LAUNCHER_PID_FILE}" "previous one-click launcher"
  stop_pid_file_process "${UI_PID_FILE}" "previous Mission Control UI"
  stop_pid_file_process "${GATEWAY_PID_FILE}" "previous gateway"
  reclaim_repo_runtime_processes
  reclaim_repo_listener_on_port "${GATEWAY_PORT}" "gateway"
  reclaim_repo_listener_on_port "${UI_PORT}" "Mission Control UI"
  if [[ "${UI_PORT}" != "1420" ]]; then
    reclaim_repo_listener_on_port 1420 "Mission Control UI"
  fi
  cleanup_oneclick_runtime_residue
}

find_free_port() {
  local preferred="$1"
  python3 - "$preferred" <<'PY'
import socket
import sys

start = int(sys.argv[1])
for port in range(start, start + 2000):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            s.bind(("127.0.0.1", port))
            print(port)
            raise SystemExit(0)
        except OSError:
            pass

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
PY
}

generate_token() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 24
    return
  fi
  python3 - <<'PY'
import secrets
print(secrets.token_hex(24))
PY
}

dotenv_quote() {
  local value="$1"
  value="${value//$'\\'/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/}"
  printf '"%s"' "${value}"
}

mask_secret() {
  local value="$1"
  local len=${#value}
  if (( len <= 8 )); then
    printf '********'
    return
  fi
  printf '%s******%s' "${value:0:4}" "${value: -4}"
}

ensure_mission_control_deps() {
  local reason=""
  local expected_bin="${MC_APP_DIR}/node_modules/.bin/vite"

  if [[ "${MODE}" == "tauri" ]]; then
    expected_bin="${MC_APP_DIR}/node_modules/.bin/tauri"
  fi

  if [[ ! -d "${MC_APP_DIR}/node_modules" ]]; then
    reason="node_modules missing"
  elif [[ ! -x "${expected_bin}" ]]; then
    reason="required local CLI missing"
  fi

  if [[ -z "${reason}" ]]; then
    return 0
  fi

  echo "Bootstrapping Mission Control dependencies (${reason})..."
  local install_cmd=(npm install)
  if [[ -f "${MC_APP_DIR}/package-lock.json" ]]; then
    install_cmd=(npm ci)
  fi

  (
    cd "${MC_APP_DIR}"
    "${install_cmd[@]}"
  ) >>"${BOOTSTRAP_LOG}" 2>&1 || {
    echo "Mission Control dependency bootstrap failed. Last bootstrap log lines:" >&2
    tail -n 80 "${BOOTSTRAP_LOG}" >&2 || true
    exit 1
  }
}

write_mission_control_env_file() {
  if [[ -f "${MC_ENV_FILE}" ]]; then
    MC_ENV_BACKUP="$(mktemp "${STATE_DIR}/mc-env-backup.XXXXXX")"
    cp "${MC_ENV_FILE}" "${MC_ENV_BACKUP}"
  fi
  local escaped_gateway_url escaped_token
  escaped_gateway_url="$(dotenv_quote "${GATEWAY_URL}")"
  escaped_token="$(dotenv_quote "${TOKEN}")"
  cat > "${MC_ENV_FILE}" <<EOF
VITE_CARSINOS_GATEWAY_URL=${escaped_gateway_url}
VITE_CARSINOS_GATEWAY_TOKEN=${escaped_token}
VITE_CARSINOS_PREFER_ENV_TOKEN=true
EOF
}

gateway_log_has_stale_trust_lock() {
  [[ -f "${GATEWAY_LOG}" ]] || return 1
  grep -q "runtime trust lock hash mismatch" "${GATEWAY_LOG}" 2>/dev/null
}

start_gateway_process() {
  echo "Starting gateway..."
  (
    export CARSINOS_GATEWAY_BIND="${GATEWAY_HOST}:${GATEWAY_PORT_SELECTED}"
    export CARSINOS_GATEWAY_TOKEN="${TOKEN}"
    export CARSINOS_STATE_DIR="${STATE_DIR}"
    export CARSINOS_LEGACY_LAUNCH_PROFILE="development"
    export CARSINOS_SECRET_STORE="${CARSINOS_SECRET_STORE:-file}"
    export CARSINOS_SECRET_FILE_DIR="${CARSINOS_SECRET_FILE_DIR:-${STATE_DIR}/secrets}"
    export CARSINOS_NUMQUAM_MANAGED_REPO_ROOT="${CARSINOS_NUMQUAM_MANAGED_REPO_ROOT:-${REPO_ROOT}}"
    export CARSINOS_NUMQUAM_MANAGED_LANES_ROOT="${CARSINOS_NUMQUAM_MANAGED_LANES_ROOT:-${MNO_LANES_DIR}}"
    exec cargo run -p carsinos-gateway
  ) >"${GATEWAY_LOG}" 2>&1 &
  GATEWAY_PID="$!"
  write_pid_file "${GATEWAY_PID_FILE}" "${GATEWAY_PID}"
}

restart_gateway_after_stale_trust_lock() {
  echo "Detected stale one-click trust lock. Clearing launcher residue and retrying gateway startup..."
  stop_pid_tree_if_running "${GATEWAY_PID}" "stale gateway"
  cleanup_oneclick_runtime_residue
  mkdir -p "${STATE_DIR}/logs" "${PID_DIR}"
  : >"${GATEWAY_LOG}"
  start_gateway_process
}

wait_for_gateway_health() {
  echo "Waiting for gateway health..."
  GATEWAY_URL="http://${GATEWAY_HOST}:${GATEWAY_PORT_SELECTED}"
  HEALTH_URL="${GATEWAY_URL}/api/v1/health"

  for _ in $(seq 1 90); do
    if curl -fsS -H "Authorization: Bearer ${TOKEN}" "${HEALTH_URL}" >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "${GATEWAY_PID}" ]] && ! kill -0 "${GATEWAY_PID}" >/dev/null 2>&1; then
      if [[ "${GATEWAY_START_ATTEMPTED_RECOVERY}" == "0" ]] && gateway_log_has_stale_trust_lock; then
        GATEWAY_START_ATTEMPTED_RECOVERY=1
        restart_gateway_after_stale_trust_lock
        continue
      fi
      echo "Gateway exited before becoming healthy. Last gateway log lines:" >&2
      tail -n 80 "${GATEWAY_LOG}" >&2 || true
      return 1
    fi
    sleep 1
  done

  if curl -fsS -H "Authorization: Bearer ${TOKEN}" "${HEALTH_URL}" >/dev/null 2>&1; then
    return 0
  fi

  if [[ -n "${GATEWAY_PID}" ]] && ! kill -0 "${GATEWAY_PID}" >/dev/null 2>&1; then
    echo "Gateway exited before becoming healthy. Last gateway log lines:" >&2
  else
    echo "Gateway did not become healthy. Last gateway log lines:" >&2
  fi
  tail -n 80 "${GATEWAY_LOG}" >&2 || true
  return 1
}

restore_mission_control_env_file() {
  if [[ -n "${MC_ENV_BACKUP}" ]] && [[ -f "${MC_ENV_BACKUP}" ]]; then
    cp "${MC_ENV_BACKUP}" "${MC_ENV_FILE}" || true
    rm -f "${MC_ENV_BACKUP}" || true
    return
  fi
  rm -f "${MC_ENV_FILE}" || true
}

cleanup() {
  local exit_code=$?
  restore_mission_control_env_file
  cleanup_stale_gateway_listener
  cleanup_stale_mission_control_vite
  if [[ -n "${UI_PID}" ]] && kill -0 "${UI_PID}" >/dev/null 2>&1; then
    kill "${UI_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${GATEWAY_PID}" ]] && kill -0 "${GATEWAY_PID}" >/dev/null 2>&1; then
    kill "${GATEWAY_PID}" >/dev/null 2>&1 || true
  fi
  reclaim_repo_runtime_processes
  clear_pid_file "${UI_PID_FILE}"
  clear_pid_file "${GATEWAY_PID_FILE}"
  clear_pid_file "${LAUNCHER_PID_FILE}"
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

while [[ $# -gt 0 ]]; do
  case "$1" in
    --web)
      MODE="web"
      shift
      ;;
    --tauri)
      MODE="tauri"
      shift
      ;;
    --gateway-port)
      GATEWAY_PORT="${2:-}"
      shift 2
      ;;
    --ui-port)
      UI_PORT="${2:-}"
      shift 2
      ;;
    --gateway-host)
      GATEWAY_HOST="${2:-}"
      shift 2
      ;;
    --token)
      TOKEN="${2:-}"
      shift 2
      ;;
    --validate-only)
      VALIDATE_ONLY=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

require_cmd python3
assert_development_state_root
if [[ "${VALIDATE_ONLY}" == "1" ]]; then
  echo "EA406 development-state fence accepted: ${STATE_DIR}"
  exit 0
fi

require_cmd cargo
require_cmd npm
require_cmd curl
require_cmd lsof
require_cmd pgrep

reclaim_previous_oneclick_runtime
write_pid_file "${LAUNCHER_PID_FILE}" "$$"

if [[ -z "${TOKEN}" ]]; then
  if [[ -t 0 ]]; then
    read -r -p "Gateway token [Enter=auto-generate]: " TOKEN
  fi
  TOKEN="${TOKEN:-$(generate_token)}"
  echo "Using generated gateway token."
fi

GATEWAY_PORT_SELECTED="$(find_free_port "${GATEWAY_PORT}")"
if [[ "${GATEWAY_PORT_SELECTED}" != "${GATEWAY_PORT}" ]]; then
  echo "Gateway port ${GATEWAY_PORT} is busy; using ${GATEWAY_PORT_SELECTED}."
fi

UI_PORT_SELECTED="${UI_PORT}"
if [[ "${MODE}" == "web" ]]; then
  UI_PORT_SELECTED="$(find_free_port "${UI_PORT}")"
  if [[ "${UI_PORT_SELECTED}" != "${UI_PORT}" ]]; then
    echo "UI port ${UI_PORT} is busy; using ${UI_PORT_SELECTED}."
  fi
else
  cleanup_stale_mission_control_vite
  if is_port_in_use 1420; then
    echo "Tauri mode requires port 1420, but it is currently in use." >&2
    echo "Current listener(s) on 1420:" >&2
    lsof -nP -iTCP:1420 -sTCP:LISTEN >&2 || true
    echo "Close the existing listener and retry, or use --web mode." >&2
    exit 1
  fi
fi

mkdir -p "${STATE_DIR}/logs"
GATEWAY_LOG="${STATE_DIR}/logs/gateway-oneclick.log"
UI_LOG="${STATE_DIR}/logs/mission-control-oneclick.log"
BOOTSTRAP_LOG="${STATE_DIR}/logs/mission-control-bootstrap.log"

: >"${BOOTSTRAP_LOG}"
ensure_mission_control_deps

start_gateway_process

if ! wait_for_gateway_health; then
  exit 1
fi

echo "Gateway ready: ${GATEWAY_URL}"
echo "Gateway token: $(mask_secret "${TOKEN}")"
echo "Gateway log: ${GATEWAY_LOG}"
write_mission_control_env_file

if [[ "${MODE}" == "tauri" ]]; then
  if [[ "${UI_PORT_SELECTED}" != "1420" ]]; then
    echo "Tauri mode requires dev port 1420 due tauri.conf devUrl. Use --web for auto UI ports." >&2
    exit 1
  fi
  echo "Starting Mission Control (tauri dev)..."
  (
    cd "${MC_APP_DIR}"
    exec npm run tauri:dev
  ) >"${UI_LOG}" 2>&1 &
  UI_PID="$!"
  write_pid_file "${UI_PID_FILE}" "${UI_PID}"
else
  echo "Starting Mission Control (web)..."
  (
    cd "${MC_APP_DIR}"
    exec npm run dev -- --host 127.0.0.1 --port "${UI_PORT_SELECTED}"
  ) >"${UI_LOG}" 2>&1 &
  UI_PID="$!"
  write_pid_file "${UI_PID_FILE}" "${UI_PID}"
  UI_URL="http://127.0.0.1:${UI_PORT_SELECTED}"
  echo "Mission Control URL: ${UI_URL}"
  if command -v open >/dev/null 2>&1; then
    open "${UI_URL}" >/dev/null 2>&1 || true
  fi
fi

echo "Mission Control log: ${UI_LOG}"
echo "Press Ctrl+C to stop."
wait "${UI_PID}"
