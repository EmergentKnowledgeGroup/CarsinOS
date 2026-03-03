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
MC_APP_DIR="${REPO_ROOT}/apps/mission-control"
MC_ENV_FILE="${MC_APP_DIR}/.env.development.local"
MC_ENV_BACKUP=""

GATEWAY_PID=""
UI_PID=""

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

is_port_in_use() {
  local port="$1"
  lsof -nP -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
}

cleanup_stale_mission_control_vite() {
  local pids pid cmd
  pids="$(lsof -t -nP -iTCP:1420 -sTCP:LISTEN 2>/dev/null || true)"
  for pid in ${pids}; do
    cmd="$(ps -p "${pid}" -o command= 2>/dev/null || true)"
    if [[ "${cmd}" == *"${MC_APP_DIR}"* ]] && [[ "${cmd}" == *"vite"* ]]; then
      kill "${pid}" >/dev/null 2>&1 || true
    fi
  done
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
  cleanup_stale_mission_control_vite
  if [[ -n "${UI_PID}" ]] && kill -0 "${UI_PID}" >/dev/null 2>&1; then
    kill "${UI_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${GATEWAY_PID}" ]] && kill -0 "${GATEWAY_PID}" >/dev/null 2>&1; then
    kill "${GATEWAY_PID}" >/dev/null 2>&1 || true
  fi
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

require_cmd cargo
require_cmd npm
require_cmd curl
require_cmd python3
require_cmd lsof

if [[ -z "${TOKEN}" ]]; then
  if [[ -t 0 ]]; then
    read -r -p "Gateway token (leave empty to auto-generate): " TOKEN
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

echo "Starting gateway..."
(
  export CARSINOS_GATEWAY_BIND="${GATEWAY_HOST}:${GATEWAY_PORT_SELECTED}"
  export CARSINOS_GATEWAY_TOKEN="${TOKEN}"
  export CARSINOS_STATE_DIR="${STATE_DIR}"
  cargo run -p carsinos-gateway
) >"${GATEWAY_LOG}" 2>&1 &
GATEWAY_PID="$!"

echo "Waiting for gateway health..."
GATEWAY_URL="http://${GATEWAY_HOST}:${GATEWAY_PORT_SELECTED}"
HEALTH_URL="${GATEWAY_URL}/api/v1/health"
for _ in $(seq 1 90); do
  if curl -fsS -H "Authorization: Bearer ${TOKEN}" "${HEALTH_URL}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -fsS -H "Authorization: Bearer ${TOKEN}" "${HEALTH_URL}" >/dev/null 2>&1; then
  echo "Gateway did not become healthy. Last gateway log lines:" >&2
  tail -n 60 "${GATEWAY_LOG}" >&2 || true
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
    npm run tauri:dev
  ) >"${UI_LOG}" 2>&1 &
  UI_PID="$!"
else
  echo "Starting Mission Control (web)..."
  (
    cd "${MC_APP_DIR}"
    npm run dev -- --host 127.0.0.1 --port "${UI_PORT_SELECTED}"
  ) >"${UI_LOG}" 2>&1 &
  UI_PID="$!"
  UI_URL="http://127.0.0.1:${UI_PORT_SELECTED}"
  echo "Mission Control URL: ${UI_URL}"
  if command -v open >/dev/null 2>&1; then
    open "${UI_URL}" >/dev/null 2>&1 || true
  fi
fi

echo "Mission Control log: ${UI_LOG}"
echo "Press Ctrl+C to stop."
wait "${UI_PID}"
