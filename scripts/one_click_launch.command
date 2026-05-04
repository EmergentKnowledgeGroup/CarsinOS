#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

MODE="--tauri"
if [[ -t 0 ]]; then
  echo "Launch mode:"
  echo "  1) Desktop app (Tauri) [default]"
  echo "  2) Browser (web)"
  read -r -p "Choose 1 or 2 [Enter=1]: " selection
  case "${selection:-1}" in
    1) MODE="--tauri" ;;
    2) MODE="--web" ;;
    *) MODE="--tauri" ;;
  esac
fi

if ! bash "${REPO_ROOT}/scripts/one_click_launch.sh" "${MODE}"; then
  echo
  echo "Launch failed. Check logs under runtime/oneclick-state/logs/."
  echo "Press Enter to close."
  read -r _
  exit 1
fi
