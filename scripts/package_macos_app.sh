#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS packaging is only supported on Darwin hosts" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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

echo "[packaging] building binaries (${PROFILE})"
if [[ "${BUILD_RELEASE}" == "true" ]]; then
  cargo build -p carsinos-gateway -p carsinos-gui --release
else
  cargo build -p carsinos-gateway -p carsinos-gui
fi

APP_NAME="carsinOS.app"
APP_ROOT="${OUT_DIR}/${APP_NAME}"
MACOS_DIR="${APP_ROOT}/Contents/MacOS"
HELPERS_DIR="${APP_ROOT}/Contents/Helpers"
RESOURCES_DIR="${APP_ROOT}/Contents/Resources"

rm -rf -- "${APP_ROOT}"
mkdir -p "${MACOS_DIR}" "${HELPERS_DIR}" "${RESOURCES_DIR}"

cp "${ROOT_DIR}/target/${PROFILE}/carsinos-gui" "${MACOS_DIR}/carsinos-gui"
cp "${ROOT_DIR}/target/${PROFILE}/carsinos-gateway" "${HELPERS_DIR}/carsinos-gateway"
chmod +x "${MACOS_DIR}/carsinos-gui" "${HELPERS_DIR}/carsinos-gateway"

cat > "${MACOS_DIR}/carsinos" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail

CONTENTS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
GATEWAY_BIN="${CONTENTS_DIR}/Helpers/carsinos-gateway"
GUI_BIN="${CONTENTS_DIR}/MacOS/carsinos-gui"
TOKEN="${CARSINOS_GATEWAY_TOKEN:-carsinos-local-token}"

start_gateway() {
  CARSINOS_GATEWAY_TOKEN="${TOKEN}" nohup "${GATEWAY_BIN}" >/tmp/carsinos-gateway.log 2>&1 &
}

if command -v nc >/dev/null 2>&1; then
  if ! nc -z 127.0.0.1 18789 >/dev/null 2>&1; then
    start_gateway
    sleep 0.4
  fi
else
  start_gateway
  sleep 0.4
fi

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
  <string>com.carsinos.desktop</string>
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

echo "[packaging] app bundle created: ${APP_ROOT}"
echo "[packaging] launch binary: ${APP_ROOT}/Contents/MacOS/carsinos"
