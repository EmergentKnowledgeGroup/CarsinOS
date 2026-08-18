$ErrorActionPreference = "Stop"

$desktopPattern = 'Z:\\modelNumquamOblita\\app\\desktop'
$runtimePattern = 'Z:\\modelNumquamOblita\\tools\\run_live_runtime.py'
$targets = Get-CimInstance Win32_Process | Where-Object {
    ($_.CommandLine -match $desktopPattern) -or ($_.CommandLine -match $runtimePattern)
}

if (-not $targets) {
    Write-Output "No matching ModelNumquamOblita desktop processes found."
    exit 0
}

$stopped = 0
foreach ($proc in $targets) {
    try {
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
        Write-Output ("Stopped PID " + $proc.ProcessId)
        $stopped++
    }
    catch {
        Write-Output ("Failed to stop PID " + $proc.ProcessId + ": " + $_.Exception.Message)
    }
}

Write-Output ("Stopped " + $stopped + " ModelNumquamOblita desktop process(es).")
