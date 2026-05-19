#!/usr/bin/env python3
from __future__ import annotations

import platform
import subprocess
import sys


DESKTOP_PATTERN = r"Z:\\modelNumquamOblita\\app\\desktop"
RUNTIME_PATTERN = r"Z:\\modelNumquamOblita\\tools\\run_live_runtime.py"


def main() -> int:
    if platform.system().lower() != "windows":
        print("This helper is meant to be run from native Windows PowerShell or Command Prompt.")
        print(r"Use it from: Z:\modelNumquamOblita")
        return 1

    ps_script = rf"""
$targets = Get-CimInstance Win32_Process |
  Where-Object {{
    ($_.CommandLine -match '{DESKTOP_PATTERN}') -or
    ($_.CommandLine -match '{RUNTIME_PATTERN}')
  }}

if (-not $targets) {{
  Write-Output 'No matching ModelNumquamOblita desktop processes found.'
  exit 0
}}

$count = 0
foreach ($proc in $targets) {{
  try {{
    Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
    Write-Output ("Stopped PID " + $proc.ProcessId)
    $count++
  }} catch {{
    Write-Output ("Failed to stop PID " + $proc.ProcessId + ": " + $_.Exception.Message)
  }}
}}

Write-Output ("Stopped " + $count + " ModelNumquamOblita desktop process(es).")
"""

    completed = subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps_script],
        text=True,
        capture_output=True,
    )

    if completed.stdout.strip():
        print(completed.stdout.strip())
    if completed.stderr.strip():
        print(completed.stderr.strip(), file=sys.stderr)
    return int(completed.returncode)


if __name__ == "__main__":
    raise SystemExit(main())
