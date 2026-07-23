param(
  [Parameter(Mandatory = $true)]
  [string]$MsiPath,
  [int]$GatewayPort = 18789
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"
$MsiPath = [IO.Path]::GetFullPath($MsiPath)
if (-not (Test-Path -LiteralPath $MsiPath -PathType Leaf)) { throw "MSI not found: $MsiPath" }

$StateRoot = Join-Path $env:LOCALAPPDATA "io.carsinos.missioncontrol\state"
$StateParent = Split-Path -Parent $StateRoot
$StateExistedBefore = Test-Path -LiteralPath $StateRoot
if ($StateExistedBefore) {
  throw "Refusing clean-install test because product state already exists: $StateRoot"
}

function Assert([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw "ASSERTION FAILED: $Message" }
}

function Test-Port([int]$Port) {
  $client = New-Object Net.Sockets.TcpClient
  try {
    $result = $client.BeginConnect("127.0.0.1", $Port, $null, $null)
    if (-not $result.AsyncWaitHandle.WaitOne(500)) { return $false }
    $client.EndConnect($result)
    return $true
  } catch {
    return $false
  } finally {
    $client.Dispose()
  }
}

function Get-ProductEntry {
  $roots = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*"
  )
  foreach ($root in $roots) {
    $match = Get-ItemProperty $root -ErrorAction SilentlyContinue | Where-Object {
      $_.PSObject.Properties["DisplayName"] -and
        $_.PSObject.Properties["DisplayName"].Value -eq "CarsinOS Mission Control"
    } | Select-Object -First 1
    if ($match) { return $match }
  }
  return $null
}

function Get-PropertyValue([object]$Object, [string]$Name) {
  if ($null -eq $Object) { return $null }
  $property = $Object.PSObject.Properties[$Name]
  if ($null -eq $property) { return $null }
  return $property.Value
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds, [string]$Failure) {
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if (& $Condition) { return }
    Start-Sleep -Milliseconds 500
  }
  throw $Failure
}

Assert (-not (Test-Port $GatewayPort)) "gateway port $GatewayPort is already occupied"
Assert ($null -eq (Get-ProductEntry)) "CarsinOS Mission Control is already installed"

$installed = $false
$appProcess = $null
try {
  # Tauri's WiX bundle defaults to a machine install, but supports the standard
  # dual-purpose MSI properties. Exercise the no-admin current-user path used by
  # the beta instructions and CI clean-host test.
  $install = Start-Process msiexec.exe -ArgumentList @(
    "/i", "`"$MsiPath`"", "/qn", "/norestart", "ALLUSERS=2", "MSIINSTALLPERUSER=1"
  ) -Wait -PassThru
  Assert ($install.ExitCode -eq 0) "MSI install failed with exit code $($install.ExitCode)"
  $installed = $true
  $entry = Get-ProductEntry
  Assert ($null -ne $entry) "installer did not register CarsinOS Mission Control"

  $candidates = @()
  $installLocation = Get-PropertyValue $entry "InstallLocation"
  $displayIcon = Get-PropertyValue $entry "DisplayIcon"
  if ($installLocation) { $candidates += Join-Path ([string]$installLocation) "carsinos-mission-control.exe" }
  if ($displayIcon) { $candidates += ([string]$displayIcon).Trim('"').Split(",")[0] }
  $candidates += @(
    (Join-Path $env:LOCALAPPDATA "CarsinOS Mission Control\carsinos-mission-control.exe"),
    (Join-Path $env:LOCALAPPDATA "Programs\CarsinOS Mission Control\carsinos-mission-control.exe")
  )
  $appExe = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
  Assert (-not [string]::IsNullOrWhiteSpace($appExe)) "installed application executable was not found"
  $installDir = Split-Path -Parent $appExe
  Assert (Test-Path -LiteralPath (Join-Path $installDir "carsinos-gateway.exe")) "gateway sidecar was not installed"
  Assert (Test-Path -LiteralPath (Join-Path $installDir "carsinos-effect-recorder.exe")) "effect recorder sidecar was not installed"

  $appProcess = Start-Process -FilePath $appExe -PassThru
  Wait-Until { Test-Port $GatewayPort } 30 "packaged gateway did not bind loopback port $GatewayPort"
  Wait-Until { Test-Path -LiteralPath (Join-Path $StateRoot "carsinos.db") } 30 "packaged gateway did not initialize product state"
  Assert (-not $appProcess.HasExited) "Mission Control exited during first launch"

  $fixture = Join-Path $StateRoot "lifecycle-proof.txt"
  Set-Content -LiteralPath $fixture -Value "v0.1.0-beta lifecycle proof"
  $appProcess.CloseMainWindow() | Out-Null
  Wait-Until { $appProcess.HasExited } 15 "Mission Control did not exit cleanly"
  Wait-Until { -not (Test-Port $GatewayPort) } 15 "gateway sidecar remained after Mission Control exit"

  $uninstall = Start-Process msiexec.exe -ArgumentList @(
    "/x", "`"$MsiPath`"", "/qn", "/norestart", "ALLUSERS=2", "MSIINSTALLPERUSER=1"
  ) -Wait -PassThru
  Assert ($uninstall.ExitCode -eq 0) "MSI uninstall failed with exit code $($uninstall.ExitCode)"
  $installed = $false
  Assert ($null -eq (Get-ProductEntry)) "installer registration remains after uninstall"
  Assert (-not (Test-Path -LiteralPath $appExe)) "application binary remains after uninstall"
  Assert (Test-Path -LiteralPath $fixture) "uninstall removed user-owned state"

  Write-Host "Windows beta lifecycle passed: install, bundled sidecar launch, state initialization, clean shutdown, uninstall, state preservation."
} finally {
  if ($appProcess -and -not $appProcess.HasExited) {
    $appProcess.CloseMainWindow() | Out-Null
    Start-Sleep -Seconds 2
    if (-not $appProcess.HasExited) { Stop-Process -Id $appProcess.Id -Force -ErrorAction SilentlyContinue }
  }
  if ($installed) {
    Start-Process msiexec.exe -ArgumentList @(
      "/x", "`"$MsiPath`"", "/qn", "/norestart", "ALLUSERS=2", "MSIINSTALLPERUSER=1"
    ) -Wait | Out-Null
  }
  if (-not $StateExistedBefore -and (Test-Path -LiteralPath $StateRoot)) {
    $resolved = [IO.Path]::GetFullPath($StateRoot)
    $expected = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "io.carsinos.missioncontrol\state"))
    if ($resolved -eq $expected) { Remove-Item -LiteralPath $StateRoot -Recurse -Force }
  }
  if ((Test-Path -LiteralPath $StateParent) -and @(Get-ChildItem -LiteralPath $StateParent -Force).Count -eq 0) {
    Remove-Item -LiteralPath $StateParent -Force
  }
}
