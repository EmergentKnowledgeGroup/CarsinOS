param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("Backup", "Verify", "Restore", "SchemaReplace")]
  [string]$Action,
  [string]$StateDir = "",
  [string]$ArchivePath = "",
  [string]$Replacement = "",
  [string]$LaunchDisabledMarker = "",
  [string]$BinaryCompatibilityVersion = "",
  [string]$ExpectedBinaryCompatibilityVersion = "",
  [switch]$Force
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $PSCommandPath
$Python = Get-Command python -ErrorAction Stop
$arguments = @((Join-Path $ScriptDir "carsinos_state.py"), $Action.ToLowerInvariant() -replace "schemareplace", "schema_replace")
if ($StateDir) { $arguments += @("--state-dir", $StateDir) }
if ($ArchivePath) { $arguments += @("--archive-path", $ArchivePath) }
if ($Replacement) { $arguments += @("--replacement", $Replacement) }
if ($LaunchDisabledMarker) { $arguments += @("--launch-disabled-marker", $LaunchDisabledMarker) }
if ($BinaryCompatibilityVersion) { $arguments += @("--binary-compatibility-version", $BinaryCompatibilityVersion) }
if ($ExpectedBinaryCompatibilityVersion) { $arguments += @("--expected-binary-compatibility-version", $ExpectedBinaryCompatibilityVersion) }
if ($Force) { $arguments += "--force" }
& $Python.Source @arguments
exit $LASTEXITCODE
