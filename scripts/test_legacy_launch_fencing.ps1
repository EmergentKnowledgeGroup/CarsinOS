param(
  [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

function Invoke-LauncherValidation([string]$StateDir) {
  $launcher = Join-Path $RepoRoot "scripts\one_click_launch.ps1"
  $previousErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $launcher -Web -NoCodexBridge -NoOpen -StateDir $StateDir -ValidateOnly 2>&1
  } finally {
    $ErrorActionPreference = $previousErrorActionPreference
  }
  return [pscustomobject]@{ ExitCode = $LASTEXITCODE; Output = ($output | Out-String) }
}

$launcher = Join-Path $RepoRoot "scripts\one_click_launch.ps1"
$shellLauncher = Join-Path $RepoRoot "scripts\one_click_launch.sh"
$macPackage = Join-Path $RepoRoot "scripts\package_macos_app.sh"

$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($launcher, [ref]$null, [ref]$parseErrors)
Assert-True ($parseErrors.Count -eq 0) "PowerShell launcher has parse errors: $($parseErrors | Out-String)"

$productionRoot = Join-Path $env:LOCALAPPDATA "io.carsinos.missioncontrol\state"
$rejected = Invoke-LauncherValidation $productionRoot
Assert-True ($rejected.ExitCode -ne 0) "canonical production state root unexpectedly passed one-click validation"
Assert-True ($rejected.Output -match "canonical Mission Control production\s+state root") "canonical-root rejection did not explain the fence"

$developmentRoot = Join-Path $RepoRoot "runtime\ea406-development-state"
$accepted = Invoke-LauncherValidation $developmentRoot
Assert-True ($accepted.ExitCode -eq 0) "separate development state root did not pass one-click validation: $($accepted.Output)"
Assert-True ($accepted.Output -match "EA406 development-state fence accepted") "development-root validation did not report acceptance"

$shellSource = Get-Content -Raw -LiteralPath $shellLauncher
Assert-True ($shellSource -match "assert_development_state_root") "shell launcher does not enforce the development-state fence"
Assert-True ($shellSource -match 'CARSINOS_LEGACY_LAUNCH_PROFILE="development"') "shell launcher does not label child gateway launches as development"

$macPackageSource = Get-Content -Raw -LiteralPath $macPackage
Assert-True ($macPackageSource -match "CARSINOS_LEGACY_GUI_DEVELOPMENT_STATE_ROOT") "legacy macOS package helper has no explicit development state root"
Assert-True ($macPackageSource -notmatch "nc -z 127\.0\.0\.1 18789") "legacy macOS package helper still uses port presence as launch authority"

Write-Output "EA406 launcher fencing tests passed."
