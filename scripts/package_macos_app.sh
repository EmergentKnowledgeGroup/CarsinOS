#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS packaging is only supported on Darwin hosts" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

: "${CARSINOS_APPLE_TEAM_ID:?CARSINOS_APPLE_TEAM_ID is required for Keychain custody}"
: "${CARSINOS_APPLE_SIGNING_IDENTITY:?CARSINOS_APPLE_SIGNING_IDENTITY is required for Keychain custody}"
if [[ ! "${CARSINOS_APPLE_TEAM_ID}" =~ ^[A-Z0-9]{10}$ ]]; then
  echo "error: CARSINOS_APPLE_TEAM_ID must be the ten-character Apple Team ID" >&2
  exit 1
fi
export CARSINOS_KEYCHAIN_ACCESS_GROUP="${CARSINOS_APPLE_TEAM_ID}.io.carsinos.missioncontrol"

OUT_DIR="${ROOT_DIR}/target/dist"
BUILD_MODE="--release"

if [[ $# -ge 1 ]]; then
  if [[ "${1}" == "--release" || "${1}" == "--debug" ]]; then
    BUILD_MODE="${1}"
    shift
  else
    OUT_DIR="${1}"
    shift
    if [[ $# -ge 1 ]]; then
      BUILD_MODE="${1}"
      shift
    fi
  fi
fi

if [[ $# -gt 0 ]]; then
  echo "error: unexpected extra arguments: $*" >&2
  exit 1
fi

if [[ "${BUILD_MODE}" != "--release" && "${BUILD_MODE}" != "--debug" ]]; then
  echo "error: build mode must be --release or --debug" >&2
  exit 1
fi

PROFILE="release"
BUILD_RELEASE=true
if [[ "${BUILD_MODE}" == "--debug" ]]; then
  PROFILE="debug"
  BUILD_RELEASE=false
fi

cd "${ROOT_DIR}"

mkdir -p "${OUT_DIR}"
ENTITLEMENTS="${OUT_DIR}/carsinos-receipt-custody.entitlements.plist"
cp "${ROOT_DIR}/apps/mission-control/src-tauri/Entitlements.plist" "${ENTITLEMENTS}"
/usr/libexec/PlistBuddy -c "Set :keychain-access-groups:0 ${CARSINOS_KEYCHAIN_ACCESS_GROUP}" "${ENTITLEMENTS}"

echo "[packaging] building binaries (${PROFILE})"
if [[ "${BUILD_RELEASE}" == "true" ]]; then
  cargo build -p carsinos-gateway -p carsinos-gui -p carsinos-storage --bin carsinos-receipt-integrity --release
else
  cargo build -p carsinos-gateway -p carsinos-gui -p carsinos-storage --bin carsinos-receipt-integrity
fi

CARSINOS_CARGO_TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
CARSINOS_BIN_DIR="${CARSINOS_CARGO_TARGET_DIR}/${PROFILE}"

APP_NAME="carsinOS.app"
APP_ROOT="${OUT_DIR}/${APP_NAME}"
MACOS_DIR="${APP_ROOT}/Contents/MacOS"
HELPERS_DIR="${APP_ROOT}/Contents/Helpers"
RESOURCES_DIR="${APP_ROOT}/Contents/Resources"

rm -rf -- "${APP_ROOT}"
mkdir -p "${MACOS_DIR}" "${HELPERS_DIR}" "${RESOURCES_DIR}"

cp "${CARSINOS_BIN_DIR}/carsinos-gui" "${MACOS_DIR}/carsinos-gui"
cp "${CARSINOS_BIN_DIR}/carsinos-gateway" "${HELPERS_DIR}/carsinos-gateway"
cp "${CARSINOS_BIN_DIR}/carsinos-receipt-integrity" "${HELPERS_DIR}/carsinos-receipt-integrity"
chmod +x "${MACOS_DIR}/carsinos-gui" "${HELPERS_DIR}/carsinos-gateway" "${HELPERS_DIR}/carsinos-receipt-integrity"

cat > "${MACOS_DIR}/carsinos" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail

CONTENTS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
GATEWAY_BIN="${CONTENTS_DIR}/Helpers/carsinos-gateway"
GUI_BIN="${CONTENTS_DIR}/MacOS/carsinos-gui"
TOKEN="${CARSINOS_GATEWAY_TOKEN:-carsinos-local-token}"
DEV_STATE_ROOT="${CARSINOS_LEGACY_GUI_DEVELOPMENT_STATE_ROOT:-}"
PRODUCTION_STATE_ROOT="${HOME}/Library/Application Support/io.carsinos.missioncontrol/state"

if [[ -z "${DEV_STATE_ROOT}" ]]; then
  echo "The legacy macOS GUI launcher is fenced from Mission Control production state." >&2
  echo "For an isolated developer run, set CARSINOS_LEGACY_GUI_DEVELOPMENT_STATE_ROOT to a separate state root." >&2
  exit 64
fi

canonical_path() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys

print(str(Path(sys.argv[1]).expanduser().resolve(strict=False)).casefold())
PY
}

if [[ "$(canonical_path "${DEV_STATE_ROOT}")" == "$(canonical_path "${PRODUCTION_STATE_ROOT}")" ]]; then
  echo "The legacy macOS GUI launcher refuses the canonical Mission Control production state root." >&2
  exit 64
fi

start_gateway() {
  CARSINOS_GATEWAY_TOKEN="${TOKEN}" \
  CARSINOS_STATE_DIR="${DEV_STATE_ROOT}" \
  CARSINOS_LEGACY_LAUNCH_PROFILE="development" \
    nohup "${GATEWAY_BIN}" >/tmp/carsinos-gateway.log 2>&1 &
}

start_gateway
sleep 0.4

CARSINOS_GATEWAY_TOKEN="${TOKEN}" exec "${GUI_BIN}"
LAUNCHER

chmod +x "${MACOS_DIR}/carsinos"

cat > "${APP_ROOT}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>carsinOS</string>
  <key>CFBundleExecutable</key>
  <string>carsinos</string>
  <key>CFBundleIdentifier</key>
  <string>io.carsinos.missioncontrol</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>carsinOS</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

echo "APPLCARO" > "${APP_ROOT}/Contents/PkgInfo"

echo "[packaging] signing receipt-custody binaries and app with one access group"
codesign --force --options runtime --timestamp --sign "${CARSINOS_APPLE_SIGNING_IDENTITY}" --entitlements "${ENTITLEMENTS}" "${HELPERS_DIR}/carsinos-gateway"
codesign --force --options runtime --timestamp --sign "${CARSINOS_APPLE_SIGNING_IDENTITY}" --entitlements "${ENTITLEMENTS}" "${HELPERS_DIR}/carsinos-receipt-integrity"
codesign --force --options runtime --timestamp --sign "${CARSINOS_APPLE_SIGNING_IDENTITY}" --entitlements "${ENTITLEMENTS}" "${MACOS_DIR}/carsinos-gui"
codesign --force --options runtime --timestamp --sign "${CARSINOS_APPLE_SIGNING_IDENTITY}" --entitlements "${ENTITLEMENTS}" "${APP_ROOT}"
codesign --verify --deep --strict --verbose=2 "${APP_ROOT}"

echo "[packaging] app bundle created: ${APP_ROOT}"
echo "[packaging] launch binary: ${APP_ROOT}/Contents/MacOS/carsinos"
