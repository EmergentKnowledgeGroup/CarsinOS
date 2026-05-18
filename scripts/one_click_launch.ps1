param(
  [switch]$Web,
  [switch]$Tauri,
  [int]$GatewayPort = 0,
  [int]$UiPort = 0,
  [string]$GatewayHost = "127.0.0.1",
  [string]$GatewayConnectHost = "",
  [string]$Token = "",
  [string]$StateDir = "",
  [string]$CargoTargetDir = "",
  [int]$CodexBridgePort = 0,
  [switch]$NoCodexBridge,
  [switch]$NoOpen,
  [int]$SmokeSeconds = 0,
  [switch]$Help
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

function Show-Usage {
  @"
Usage: powershell -ExecutionPolicy Bypass -File scripts\one_click_launch.ps1 [options]

Options:
  -Web                     Launch Mission Control in browser mode.
  -Tauri                   Launch Mission Control using tauri dev (requires port 1420 free).
  -GatewayPort <port>      Preferred gateway port (default: 18789; falls forward if busy).
  -UiPort <port>           Preferred UI port in web mode (default: 1420; falls forward if busy).
  -GatewayHost <host>      Gateway host bind (default: 127.0.0.1).
  -GatewayConnectHost <h>  Gateway host used by launcher/UI checks (default: bind host, or 127.0.0.1 for wildcard binds).
  -Token <value>           Use explicit gateway token.
  -StateDir <path>         Runtime state/log/pid directory (default: runtime\oneclick-state).
  -CargoTargetDir <path>   Cargo target dir (default: <StateDir>\cargo-target).
  -CodexBridgePort <port>  Preferred Codex bridge port (default: 17889; falls forward if busy).
  -NoCodexBridge           Do not start the local Codex CLI/App bridge sidecar.
  -NoOpen                  Do not open the browser in web mode.
  -SmokeSeconds <seconds>  Stop automatically after launch stays healthy for the given seconds.
  -Help                    Show this help.

If neither -Web nor -Tauri is supplied, the launcher prompts for the same
Desktop/Web choice as scripts\one_click_launch.cmd.
"@
}

if ($Help) {
  Show-Usage
  exit 0
}

$ScriptDir = Split-Path -Parent $PSCommandPath
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path

function Resolve-LauncherPath([string]$Path) {
  if ([IO.Path]::IsPathRooted($Path)) {
    return [IO.Path]::GetFullPath($Path)
  }
  return [IO.Path]::GetFullPath((Join-Path $RepoRoot $Path))
}

function Select-LaunchMode {
  if ($Web -and $Tauri) {
    throw "Choose either -Web or -Tauri, not both."
  }
  if ($Tauri) { return "tauri" }
  if ($Web) { return "web" }

  if (-not [Environment]::UserInteractive) {
    Write-Host "No launch mode supplied in a non-interactive session; defaulting to Desktop app (Tauri)."
    return "tauri"
  }

  Write-Host "Launch mode:"
  Write-Host "  1) Desktop app (Tauri) [default]"
  Write-Host "  2) Browser (web)"
  $choice = (Read-Host "Choose 1 or 2 [Enter=1]").Trim()
  if ($choice -eq "2") { return "web" }
  return "tauri"
}

$Mode = Select-LaunchMode

if ($GatewayPort -le 0) {
  if ($env:CARSINOS_LAUNCH_GATEWAY_PORT) { $GatewayPort = [int]$env:CARSINOS_LAUNCH_GATEWAY_PORT }
  else { $GatewayPort = 18789 }
}
if ($UiPort -le 0) {
  if ($env:CARSINOS_LAUNCH_UI_PORT) { $UiPort = [int]$env:CARSINOS_LAUNCH_UI_PORT }
  else { $UiPort = 1420 }
}
if ($CodexBridgePort -le 0) {
  if ($env:CARSINOS_CODEX_BRIDGE_PORT) { $CodexBridgePort = [int]$env:CARSINOS_CODEX_BRIDGE_PORT }
  else { $CodexBridgePort = 17889 }
}
if (-not $StateDir) {
  if ($env:CARSINOS_STATE_DIR) { $StateDir = $env:CARSINOS_STATE_DIR }
  else { $StateDir = Join-Path $RepoRoot "runtime\oneclick-state" }
}
if (-not $CargoTargetDir) {
  if ($env:CARSINOS_ONECLICK_CARGO_TARGET_DIR) { $CargoTargetDir = $env:CARSINOS_ONECLICK_CARGO_TARGET_DIR }
  elseif ($env:CARGO_TARGET_DIR) { $CargoTargetDir = $env:CARGO_TARGET_DIR }
  else { $CargoTargetDir = Join-Path $StateDir "cargo-target" }
}
$StateDir = Resolve-LauncherPath $StateDir
$CargoTargetDir = Resolve-LauncherPath $CargoTargetDir
if (-not $Token -and $env:CARSINOS_GATEWAY_TOKEN) {
  $Token = $env:CARSINOS_GATEWAY_TOKEN
}

$MissionControlDir = Join-Path $RepoRoot "apps\mission-control"
$CodexBridgeDir = Join-Path $RepoRoot "tools\codex-bridge"
$CodexBridgeServer = Join-Path $CodexBridgeDir "relay\server.js"
$PidDir = Join-Path $StateDir "pids"
$LogDir = Join-Path $StateDir "logs"
$ScriptOutDir = Join-Path $StateDir "launcher-scripts"
$LauncherPidFile = Join-Path $PidDir "oneclick-launcher.pid"
$GatewayPidFile = Join-Path $PidDir "gateway.pid"
$CodexBridgePidFile = Join-Path $PidDir "codex-bridge.pid"
$UiPidFile = Join-Path $PidDir "mission-control-ui.pid"
$GatewayLog = Join-Path $LogDir "gateway-oneclick.log"
$CodexBridgeLog = Join-Path $LogDir "codex-bridge-oneclick.log"
$UiLog = Join-Path $LogDir "mission-control-oneclick.log"
$BootstrapLog = Join-Path $LogDir "mission-control-bootstrap.log"
$GatewayProcess = $null
$CodexBridgeProcess = $null
$UiProcess = $null
$GatewayPortSelected = $GatewayPort
$CodexBridgePortSelected = $CodexBridgePort
$UiPortSelected = $UiPort
$GatewayUrl = ""
$CodexBridgeUrl = ""

if (-not $NoCodexBridge -and -not (Test-Path -LiteralPath $CodexBridgeServer)) {
  Write-Host "Codex bridge source is not installed; continuing without the Codex bridge."
  $NoCodexBridge = $true
}

function Require-Command([string]$Name) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "Missing required command: $Name"
  }
}

function ConvertTo-PsLiteral([string]$Value) {
  "'" + ($Value -replace "'", "''") + "'"
}

function Get-CommandLine([int]$ProcessId) {
  try {
    $proc = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction Stop
    if ($proc) { return [string]$proc.CommandLine }
  } catch {
  }
  ""
}

function Get-ProcessInfo([int]$ProcessId) {
  try {
    return Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction Stop
  } catch {
    return $null
  }
}

function Test-RepoOwned([int]$ProcessId) {
  $seen = @{}
  $currentId = $ProcessId
  for ($depth = 0; $depth -lt 8 -and $currentId -gt 0 -and -not $seen.ContainsKey($currentId); $depth++) {
    $seen[$currentId] = $true
    $proc = Get-ProcessInfo $currentId
    if (-not $proc) { return $false }
    $cmd = [string]$proc.CommandLine
    $exe = [string]$proc.ExecutablePath
    foreach ($text in @($cmd, $exe)) {
      if (-not $text) { continue }
      if ($text.IndexOf($RepoRoot, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
          $text.IndexOf($StateDir, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
          $text.IndexOf("one_click_launch.ps1", [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
          $text.IndexOf("one_click_launch.cmd", [StringComparison]::OrdinalIgnoreCase) -ge 0) {
        return $true
      }
    }
    if ($cmd.IndexOf("cargo run -p carsinos-gateway", [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
        $cmd.IndexOf("cargo  run -p carsinos-gateway", [StringComparison]::OrdinalIgnoreCase) -ge 0) {
      return $true
    }
    $currentId = [int]$proc.ParentProcessId
  }
  return $false
}

function Get-ChildProcessIds([int]$ProcessId) {
  $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $ProcessId" -ErrorAction SilentlyContinue)
  foreach ($child in $children) {
    foreach ($descendant in Get-ChildProcessIds ([int]$child.ProcessId)) { $descendant }
    [int]$child.ProcessId
  }
}

function Stop-ProcessTree([int]$ProcessId, [string]$Label) {
  if ($ProcessId -le 0 -or $ProcessId -eq $PID) { return }
  if (-not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) { return }
  Write-Host "Reclaiming $Label (pid $ProcessId)."
  $children = @(Get-ChildProcessIds $ProcessId | Select-Object -Unique)
  foreach ($child in $children) { Stop-Process -Id $child -ErrorAction SilentlyContinue }
  Stop-Process -Id $ProcessId -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
  if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
    foreach ($child in $children) { Stop-Process -Id $child -Force -ErrorAction SilentlyContinue }
    Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
  }
}

function Get-ListenerPids([int]$Port) {
  $ids = @()
  try {
    $ids += Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction Stop |
      Select-Object -ExpandProperty OwningProcess
  } catch {
    foreach ($line in (netstat -ano -p tcp | Select-String (":$Port\s"))) {
      if ($line.Line -match "LISTENING\s+(\d+)$") { $ids += [int]$Matches[1] }
    }
  }
  $ids | Where-Object { $_ -gt 0 } | Select-Object -Unique
}

function Test-PortInUse([int]$Port) {
  @((Get-ListenerPids $Port)).Count -gt 0
}

function Reclaim-RepoPort([int]$Port, [string]$Label) {
  foreach ($listenerPid in Get-ListenerPids $Port) {
    if ($listenerPid -ne $PID -and (Test-RepoOwned $listenerPid)) {
      Stop-ProcessTree $listenerPid "$Label listener on port $Port"
    }
  }
}

function Clear-PidFile([string]$Path) {
  Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
}

function Write-PidFile([string]$Path, [int]$ProcessId) {
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  Set-Content -LiteralPath $Path -Value ([string]$ProcessId) -Encoding ASCII
}

function Stop-PidFileProcess([string]$Path, [string]$Label) {
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction SilentlyContinue
  Clear-PidFile $Path
  $processId = 0
  if ([int]::TryParse(($raw -replace "[^\d]", ""), [ref]$processId) -and (Test-RepoOwned $processId)) {
    Stop-ProcessTree $processId $Label
  }
}

function Reclaim-PreviousRuntime {
  New-Item -ItemType Directory -Force -Path $StateDir, $PidDir, $LogDir, $ScriptOutDir | Out-Null
  Remove-LauncherScripts
  Stop-PidFileProcess $LauncherPidFile "previous one-click launcher"
  Stop-PidFileProcess $UiPidFile "previous Mission Control UI"
  Stop-PidFileProcess $GatewayPidFile "previous gateway"
  Stop-PidFileProcess $CodexBridgePidFile "previous Codex bridge"
  Reclaim-RepoPort $GatewayPort "gateway"
  if (-not $NoCodexBridge) { Reclaim-RepoPort $CodexBridgePort "Codex bridge" }
  Reclaim-RepoPort $UiPort "Mission Control UI"
  if ($UiPort -ne 1420) { Reclaim-RepoPort 1420 "Mission Control UI" }
}

function Find-FreePort([int]$Preferred) {
  for ($port = $Preferred; $port -lt ($Preferred + 2000); $port++) {
    $listener = $null
    try {
      $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Parse("127.0.0.1"), $port)
      $listener.Start()
      return $port
    } catch {
    } finally {
      if ($listener) { $listener.Stop() }
    }
  }
  throw "Could not find a free port starting at $Preferred"
}

function New-GatewayToken {
  $bytes = New-Object byte[] 24
  $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
  -join ($bytes | ForEach-Object { $_.ToString("x2") })
}

function Mask-Secret([string]$Value) {
  if ($Value.Length -le 8) { return "********" }
  $Value.Substring(0, 4) + "******" + $Value.Substring($Value.Length - 4)
}

function Ensure-MissionControlDeps {
  $expected = Join-Path $MissionControlDir "node_modules\.bin\vite.cmd"
  if ($Mode -eq "tauri") { $expected = Join-Path $MissionControlDir "node_modules\.bin\tauri.cmd" }
  if ((Test-Path -LiteralPath (Join-Path $MissionControlDir "node_modules")) -and
      (Test-Path -LiteralPath $expected)) {
    return
  }
  Write-Host "Bootstrapping Mission Control dependencies..."
  Push-Location $MissionControlDir
  try {
    $env:npm_config_cache = Join-Path $StateDir "npm-cache"
    New-Item -ItemType Directory -Force -Path $env:npm_config_cache | Out-Null
    if (Test-Path -LiteralPath (Join-Path $MissionControlDir "package-lock.json")) {
      & cmd.exe /d /c "npm ci > ""$BootstrapLog"" 2>&1"
    } else {
      & cmd.exe /d /c "npm install > ""$BootstrapLog"" 2>&1"
    }
    if ($LASTEXITCODE -ne 0) {
      throw "npm bootstrap failed. Last log lines:`n$(Get-Content -LiteralPath $BootstrapLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
    }
  } finally {
    Pop-Location
  }
}

function Remove-LauncherScripts {
  if (-not (Test-Path -LiteralPath $ScriptOutDir)) { return }
  Get-ChildItem -LiteralPath $ScriptOutDir -Filter "*.ps1" -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue
}

function Start-ChildPowerShell([string]$Name, [string]$Content, [hashtable]$Environment = @{}) {
  $safeName = $Name -replace '[^A-Za-z0-9_.-]', '-'
  $path = Join-Path $ScriptOutDir "$safeName-$([Guid]::NewGuid().ToString('N')).ps1"
  [IO.File]::WriteAllText($path, $Content, [Text.UTF8Encoding]::new($false))
  $previousEnvironment = @{}
  try {
    foreach ($key in $Environment.Keys) {
      $previousEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
      [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
    }
    Start-Process -FilePath powershell.exe -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $path) -PassThru -WindowStyle Hidden
  } finally {
    foreach ($key in $Environment.Keys) {
      [Environment]::SetEnvironmentVariable($key, $previousEnvironment[$key], "Process")
    }
  }
}

function Start-CodexBridge {
  if ($NoCodexBridge) { return }
  if (-not (Test-Path -LiteralPath $CodexBridgeServer)) {
    throw "Codex bridge source is missing at $CodexBridgeDir"
  }
  Write-Host "Starting Codex bridge..."
  Set-Content -LiteralPath $CodexBridgeLog -Value "" -Encoding UTF8
  $bridgeRuntime = Join-Path $StateDir "codex-bridge"
  $allowedRoots = "$RepoRoot;$(Join-Path $StateDir "codex-bridge-workspaces");Z:\carsinos-codex-work"
  $bridgeScript = @"
`$ErrorActionPreference = "Stop"
Set-Location -LiteralPath $(ConvertTo-PsLiteral $CodexBridgeDir)
New-Item -ItemType Directory -Force -Path $(ConvertTo-PsLiteral $bridgeRuntime) | Out-Null
& node.exe "relay\server.js" > $(ConvertTo-PsLiteral $CodexBridgeLog) 2>&1
exit `$LASTEXITCODE
"@
  $Script:CodexBridgeProcess = Start-ChildPowerShell "codex-bridge-oneclick" $bridgeScript @{
    CODEX_BRIDGE_PORT = [string]$CodexBridgePortSelected
    CODEX_BRIDGE_ALLOWED_ROOTS = $allowedRoots
    CODEX_BRIDGE_RUNTIME_ROOT = $bridgeRuntime
  }
  Write-PidFile $CodexBridgePidFile $Script:CodexBridgeProcess.Id
}

function Wait-CodexBridge {
  if ($NoCodexBridge) { return }
  Write-Host "Waiting for Codex bridge..."
  $statusUrl = "$CodexBridgeUrl/status"
  for ($i = 0; $i -lt 45; $i++) {
    try {
      Invoke-WebRequest -Uri $statusUrl -UseBasicParsing -TimeoutSec 2 | Out-Null
      return
    } catch {
    }
    if ($Script:CodexBridgeProcess -and $Script:CodexBridgeProcess.HasExited) {
      throw "Codex bridge exited before becoming reachable. Last bridge log lines:`n$(Get-Content -LiteralPath $CodexBridgeLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
    }
    Start-Sleep -Seconds 1
  }
  throw "Codex bridge did not become reachable. Last bridge log lines:`n$(Get-Content -LiteralPath $CodexBridgeLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
}

function Start-Gateway {
  Write-Host "Starting gateway..."
  Set-Content -LiteralPath $GatewayLog -Value "" -Encoding UTF8
  $bind = "$GatewayHost`:$GatewayPortSelected"
  $tmpDir = Join-Path $StateDir "tmp"
  $gatewayScript = @"
`$ErrorActionPreference = "Stop"
Set-Location -LiteralPath $(ConvertTo-PsLiteral $RepoRoot)
if (-not `$env:CARSINOS_SECRET_STORE) { `$env:CARSINOS_SECRET_STORE = "file" }
if (-not `$env:CARSINOS_SECRET_FILE_DIR) { `$env:CARSINOS_SECRET_FILE_DIR = Join-Path `$env:CARSINOS_STATE_DIR "secrets" }
if (-not `$env:CARSINOS_NUMQUAM_MANAGED_REPO_ROOT) { `$env:CARSINOS_NUMQUAM_MANAGED_REPO_ROOT = $(ConvertTo-PsLiteral $RepoRoot) }
if (-not `$env:CARSINOS_NUMQUAM_MANAGED_LANES_ROOT) { `$env:CARSINOS_NUMQUAM_MANAGED_LANES_ROOT = Join-Path `$env:CARSINOS_STATE_DIR "mno-lanes" }
`$env:TMP = `$env:TEMP
New-Item -ItemType Directory -Force -Path `$env:CARGO_TARGET_DIR, `$env:TEMP | Out-Null
& cmd.exe /d /c "cargo run -p carsinos-gateway > ""`$env:CARSINOS_ONECLICK_GATEWAY_LOG"" 2>&1"
exit `$LASTEXITCODE
"@
  $Script:GatewayProcess = Start-ChildPowerShell "gateway-oneclick" $gatewayScript @{
    CARSINOS_GATEWAY_BIND = $bind
    CARSINOS_GATEWAY_TOKEN = $Token
    CARSINOS_STATE_DIR = $StateDir
    CARSINOS_CODEX_BRIDGE_BASE_URL = $CodexBridgeUrl
    CARGO_TARGET_DIR = $CargoTargetDir
    TEMP = $tmpDir
    TMP = $tmpDir
    CARSINOS_ONECLICK_GATEWAY_LOG = $GatewayLog
  }
  Write-PidFile $GatewayPidFile $Script:GatewayProcess.Id
}

function Wait-Gateway {
  Write-Host "Waiting for gateway health..."
  $headers = @{ Authorization = "Bearer $Token" }
  $healthUrl = "$GatewayUrl/api/v1/health"
  for ($i = 0; $i -lt 90; $i++) {
    try {
      Invoke-WebRequest -Uri $healthUrl -Headers $headers -UseBasicParsing -TimeoutSec 2 | Out-Null
      return
    } catch {
    }
    if ($Script:GatewayProcess -and $Script:GatewayProcess.HasExited) {
      throw "Gateway exited before becoming healthy. Last gateway log lines:`n$(Get-Content -LiteralPath $GatewayLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
    }
    Start-Sleep -Seconds 1
  }
  throw "Gateway did not become healthy. Last gateway log lines:`n$(Get-Content -LiteralPath $GatewayLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
}

function Start-MissionControl {
  $tmpDir = Join-Path $StateDir "tmp"
  if ($Mode -eq "tauri") {
    Write-Host "Starting Mission Control (tauri dev)..."
    $run = "npm run tauri:dev"
  } else {
    Write-Host "Starting Mission Control (web)..."
    $run = "npm run dev -- --host 127.0.0.1 --port $UiPortSelected"
  }
  $uiScript = @"
`$ErrorActionPreference = "Stop"
Set-Location -LiteralPath $(ConvertTo-PsLiteral $MissionControlDir)
`$env:CHOKIDAR_USEPOLLING = "true"
`$env:WATCHPACK_POLLING = "true"
`$env:VITE_CARSINOS_PREFER_ENV_TOKEN = "true"
`$env:TMP = `$env:TEMP
New-Item -ItemType Directory -Force -Path `$env:npm_config_cache, `$env:CARGO_TARGET_DIR, `$env:TEMP | Out-Null
& cmd.exe /d /c "$run > ""`$env:CARSINOS_ONECLICK_UI_LOG"" 2>&1"
exit `$LASTEXITCODE
"@
  $Script:UiProcess = Start-ChildPowerShell "mission-control-oneclick" $uiScript @{
    npm_config_cache = (Join-Path $StateDir "npm-cache")
    CARGO_TARGET_DIR = $CargoTargetDir
    VITE_CARSINOS_GATEWAY_URL = $GatewayUrl
    VITE_CARSINOS_GATEWAY_TOKEN = $Token
    TEMP = $tmpDir
    TMP = $tmpDir
    CARSINOS_ONECLICK_UI_LOG = $UiLog
  }
  Write-PidFile $UiPidFile $Script:UiProcess.Id
  if ($Mode -eq "web") {
    $uiUrl = "http://127.0.0.1:$UiPortSelected"
    Write-Host "Mission Control URL: $uiUrl"
    if (-not $NoOpen) { Start-Process $uiUrl | Out-Null }
  }
}

function Wait-WebUi {
  if ($Mode -ne "web") { return }
  $uiUrl = "http://127.0.0.1:$UiPortSelected"
  Write-Host "Waiting for Mission Control web UI..."
  for ($i = 0; $i -lt 60; $i++) {
    try {
      Invoke-WebRequest -Uri $uiUrl -UseBasicParsing -TimeoutSec 2 | Out-Null
      return
    } catch {
    }
    if ($Script:UiProcess -and $Script:UiProcess.HasExited) {
      throw "Mission Control exited before becoming reachable. Last UI log lines:`n$(Get-Content -LiteralPath $UiLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
    }
    Start-Sleep -Seconds 1
  }
  throw "Mission Control web UI did not become reachable. Last UI log lines:`n$(Get-Content -LiteralPath $UiLog -Tail 80 -ErrorAction SilentlyContinue | Out-String)"
}

function Wait-TauriUi {
  if ($Mode -ne "tauri") { return }
  Write-Host "Waiting for Mission Control desktop app..."
  for ($i = 0; $i -lt 240; $i++) {
    $desktopProcesses = @(Get-Process -Name "carsinos-mission-control" -ErrorAction SilentlyContinue |
      Where-Object { Test-RepoOwned $_.Id })
    if ($desktopProcesses.Count -gt 0) {
      $desktopProcess = $desktopProcesses[0]
      Write-Host "Mission Control desktop app is running (pid $($desktopProcess.Id))."
      if (-not $desktopProcess.MainWindowTitle) {
        Write-Host "If the window is behind another app, use Alt+Tab and select CarsinOS Mission Control."
      }
      return
    }
    if ($Script:UiProcess -and $Script:UiProcess.HasExited) {
      throw "Mission Control desktop launch exited before the app opened. Last UI log lines:`n$(Get-Content -LiteralPath $UiLog -Tail 100 -ErrorAction SilentlyContinue | Out-String)"
    }
    Start-Sleep -Seconds 1
  }
  throw "Mission Control desktop app did not open in time. Last UI log lines:`n$(Get-Content -LiteralPath $UiLog -Tail 100 -ErrorAction SilentlyContinue | Out-String)"
}

function Cleanup {
  if ($Script:UiProcess -and -not $Script:UiProcess.HasExited) { Stop-ProcessTree $Script:UiProcess.Id "Mission Control UI" }
  if ($Script:GatewayProcess -and -not $Script:GatewayProcess.HasExited) { Stop-ProcessTree $Script:GatewayProcess.Id "gateway" }
  if ($Script:CodexBridgeProcess -and -not $Script:CodexBridgeProcess.HasExited) { Stop-ProcessTree $Script:CodexBridgeProcess.Id "Codex bridge" }
  Clear-PidFile $LauncherPidFile
  Clear-PidFile $UiPidFile
  Clear-PidFile $GatewayPidFile
  Clear-PidFile $CodexBridgePidFile
  Remove-LauncherScripts
}

try {
  Require-Command cargo
  Require-Command npm
  New-Item -ItemType Directory -Force -Path $StateDir, $PidDir, $LogDir, $ScriptOutDir | Out-Null
  Set-Content -LiteralPath $BootstrapLog -Value "" -Encoding UTF8

  Reclaim-PreviousRuntime
  Write-PidFile $LauncherPidFile $PID

  if (-not $Token) {
    if ([Environment]::UserInteractive) { $Token = Read-Host "Gateway token [Enter=auto-generate]" }
    if (-not $Token) {
      $Token = New-GatewayToken
      Write-Host "Using generated gateway token."
    }
  }

  $GatewayPortSelected = Find-FreePort $GatewayPort
  if ($GatewayPortSelected -ne $GatewayPort) { Write-Host "Gateway port $GatewayPort is busy; using $GatewayPortSelected." }
  if (-not $NoCodexBridge) {
    $CodexBridgePortSelected = Find-FreePort $CodexBridgePort
    if ($CodexBridgePortSelected -ne $CodexBridgePort) { Write-Host "Codex bridge port $CodexBridgePort is busy; using $CodexBridgePortSelected." }
  }

  if ($Mode -eq "web") {
    $UiPortSelected = Find-FreePort $UiPort
    if ($UiPortSelected -ne $UiPort) { Write-Host "UI port $UiPort is busy; using $UiPortSelected." }
  } else {
    Reclaim-RepoPort 1420 "Mission Control UI"
    if (Test-PortInUse 1420) {
      $listeners = @(Get-ListenerPids 1420) -join ", "
      throw "Tauri mode requires port 1420, but it is currently in use by pid(s): $listeners. Close the listener or use -Web."
    }
  }

  $gatewayConnectHost = $GatewayConnectHost
  if (-not $gatewayConnectHost) { $gatewayConnectHost = $GatewayHost }
  if ($gatewayConnectHost -eq "0.0.0.0" -or $gatewayConnectHost -eq "::") {
    $gatewayConnectHost = "127.0.0.1"
  }
  $GatewayUrl = "http://$gatewayConnectHost`:$GatewayPortSelected"
  if (-not $NoCodexBridge) {
    $CodexBridgeUrl = "http://127.0.0.1`:$CodexBridgePortSelected"
  } else {
    $CodexBridgeUrl = ""
  }
  Ensure-MissionControlDeps
  Start-CodexBridge
  Wait-CodexBridge
  if (-not $NoCodexBridge) {
    Write-Host "Codex bridge ready: $CodexBridgeUrl"
    Write-Host "Codex bridge log: $CodexBridgeLog"
  }
  Start-Gateway
  Wait-Gateway
  Write-Host "Gateway ready: $GatewayUrl"
  Write-Host "Gateway token: $(Mask-Secret $Token)"
  Write-Host "Gateway log: $GatewayLog"

  Start-MissionControl
  Wait-WebUi
  Wait-TauriUi
  Write-Host "Mission Control log: $UiLog"
  if ($SmokeSeconds -gt 0) {
    Write-Host "Smoke mode: keeping launcher alive for $SmokeSeconds second(s)."
    Start-Sleep -Seconds $SmokeSeconds
  } else {
    Write-Host "Press Ctrl+C to stop."
    Wait-Process -Id $Script:UiProcess.Id
  }
} finally {
  Cleanup
}
