#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TOKEN="${CARSINOS_GATEWAY_TOKEN:-}"
BIND="${CARSINOS_GATEWAY_BIND:-127.0.0.1:18789}"
GATEWAY_URL=""

generate_token() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
    return 0
  fi

  python3 - <<'PY'
import secrets
print(secrets.token_hex(32))
PY
}

ensure_token() {
  if [[ -n "${TOKEN}" ]]; then
    return 0
  fi

  if [[ ! -t 0 ]]; then
    TOKEN="$(generate_token)"
    return 0
  fi

  print ""
  print "Gateway token setup"
  print "1) Generate random token (recommended)"
  print "2) Use fixed dev token (carsinos-local-token)"
  print "3) Enter token manually"

  local choice=""
  while true; do
    read -r "choice?Select [1]: "
    choice="${choice:-1}"
    case "${choice}" in
      1)
        TOKEN="$(generate_token)"
        return 0
        ;;
      2)
        TOKEN="carsinos-local-token"
        return 0
        ;;
      3)
        local manual=""
        read -r "manual?Enter token: "
        manual="${manual//[[:space:]]/}"
        if [[ -z "${manual}" ]]; then
          print "Token cannot be empty."
          continue
        fi
        TOKEN="${manual}"
        return 0
        ;;
      *)
        print "Invalid choice: ${choice}"
        ;;
    esac
  done
}

resolve_gateway_url() {
  GATEWAY_URL="http://${BIND}"
}

port_in_use() {
  local port="$1"
  lsof -iTCP:"${port}" -sTCP:LISTEN -P -n >/dev/null 2>&1
}

status_ok() {
  curl -fsS --max-time 1 -H "Authorization: Bearer ${TOKEN}" "${GATEWAY_URL}/api/v1/status" >/dev/null 2>&1
}

strip_ipv6_brackets() {
  local host="$1"
  host="${host#[}"
  host="${host%]}"
  print -- "${host}"
}

pick_ephemeral_port() {
  local host="$1"
  local host_stripped
  host_stripped="$(strip_ipv6_brackets "${host}")"
  python3 - "${host_stripped}" <<'PY'
import socket
import sys

host = sys.argv[1]
is_v6 = ":" in host
family = socket.AF_INET6 if is_v6 else socket.AF_INET
sock = socket.socket(family, socket.SOCK_STREAM)
sock.bind((host, 0, 0, 0) if is_v6 else (host, 0))
port = sock.getsockname()[1]
sock.close()
print(port)
PY
}

select_bind() {
  local host="${BIND%:*}"
  local base_port="${BIND##*:}"

  if [[ "${host}" == "${base_port}" ]]; then
    print "Invalid CARSINOS_GATEWAY_BIND (expected host:port): ${BIND}"
    return 1
  fi
  if [[ "${base_port}" != <-> ]]; then
    print "Invalid port in CARSINOS_GATEWAY_BIND: ${BIND}"
    return 1
  fi

  resolve_gateway_url

  if ! port_in_use "${base_port}"; then
    return 0
  fi

  # If the preferred bind already has a compatible carsinOS gateway (same token), reuse it.
  if status_ok; then
    return 0
  fi

  print "Port ${base_port} is already in use. Searching for a free port..."

  local offset=""
  local candidate_port=""
  for offset in {1..100}; do
    candidate_port=$((base_port + offset))
    if ! port_in_use "${candidate_port}"; then
      BIND="${host}:${candidate_port}"
      resolve_gateway_url
      print "Selected free bind: ${BIND}"
      return 0
    fi
  done

  candidate_port="$(pick_ephemeral_port "${host}")"
  BIND="${host}:${candidate_port}"
  resolve_gateway_url
  print "Selected ephemeral free bind: ${BIND}"
  return 0
}

print ""
print "Mission Control Launcher (Tauri dev)"
print "Repo: ${REPO_ROOT}"

ensure_token

print "Preferred bind: ${BIND}"
print "Gateway token: ${TOKEN}"

if command -v pbcopy >/dev/null 2>&1; then
  printf "%s" "${TOKEN}" | pbcopy
  print "Token copied to clipboard."
fi

GATEWAY_PID=""
LOG_FILE="/tmp/carsinos-gateway.log"

start_gateway() {
  select_bind
  resolve_gateway_url

  if port_in_use "${BIND##*:}"; then
    if status_ok; then
      print "Gateway already running on ${BIND}."
      return 0
    fi
  fi

  print "Starting carsinOS gateway (logs: ${LOG_FILE})..."

  (
    cd "${REPO_ROOT}"
    CARSINOS_GATEWAY_TOKEN="${TOKEN}" \
      CARSINOS_GATEWAY_BIND="${BIND}" \
      cargo run -p carsinos-gateway
  ) >"${LOG_FILE}" 2>&1 &
  GATEWAY_PID=$!

  # Best-effort readiness check.
  for _ in {1..80}; do
    if status_ok; then
      print "Gateway is up."
      return 0
    fi
    sleep 0.25
  done

  print "Gateway did not become ready in time. Check ${LOG_FILE}."
  return 1
}

cleanup() {
  if [[ -n "${GATEWAY_PID}" ]]; then
    print "Stopping gateway (pid ${GATEWAY_PID})..."
    kill "${GATEWAY_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

start_gateway

print ""
print "Next steps in the app:"
print "1. Gateway URL: ${GATEWAY_URL}"
print "2. Gateway Token: (paste; already in clipboard)"
print "3. Click: Save + Connect"
print ""

cd "${REPO_ROOT}/apps/mission-control"
if [[ ! -d node_modules ]]; then
  print "Installing npm deps..."
  npm install
fi

npm run tauri:dev
