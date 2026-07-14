param(
  [string]$Version = "v0.1.0-beta",
  [string]$OutputDir = "",
  [string]$CargoTargetDir = "",
  [switch]$SkipNpmCi,
  [switch]$AllowDirty
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $PSCommandPath
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).ProviderPath
$MissionControlDir = Join-Path $RepoRoot "apps\mission-control"
$TauriDir = Join-Path $MissionControlDir "src-tauri"
$TargetTriple = "x86_64-pc-windows-msvc"

function Resolve-RepoPath([string]$Path) {
  if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
  return [IO.Path]::GetFullPath((Join-Path $RepoRoot $Path))
}

function Assert-CommandAvailable([string]$Name) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "Missing required command: $Name" }
}

Assert-CommandAvailable cargo
Assert-CommandAvailable npm
if (-not $Version.StartsWith("v0.1.0-beta")) { throw "This release script is scoped to the v0.1.0-beta line." }

if (-not $AllowDirty) {
  $dirty = (& git -C $RepoRoot status --porcelain)
  if ($dirty) { throw "Release builds require a clean Git checkout. Use -AllowDirty only for local proof builds." }
}

if (-not $CargoTargetDir) { $CargoTargetDir = Join-Path $RepoRoot ".shared-cargo-targets\carsinos-beta" }
$CargoTargetDir = Resolve-RepoPath $CargoTargetDir
if (-not $OutputDir) { $OutputDir = Join-Path $RepoRoot "release-output\$Version\windows-x64" }
$OutputDir = Resolve-RepoPath $OutputDir
$expectedPrefix = (Join-Path $RepoRoot "release-output").TrimEnd("\") + "\"
if (-not $OutputDir.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
  throw "OutputDir must stay under $expectedPrefix"
}

$env:CARGO_TARGET_DIR = $CargoTargetDir
$env:TEMP = Join-Path $RepoRoot ".tmp"
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force -Path $env:TEMP, $CargoTargetDir | Out-Null

Write-Host "Building CarsinOS gateway sidecar..."
& cargo build --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p carsinos-gateway --release --locked
if ($LASTEXITCODE -ne 0) { throw "Gateway release build failed." }
$gatewayExe = Join-Path $CargoTargetDir "release\carsinos-gateway.exe"
if (-not (Test-Path -LiteralPath $gatewayExe)) { throw "Gateway executable not found: $gatewayExe" }
$sidecarDir = Join-Path $TauriDir "binaries"
New-Item -ItemType Directory -Force -Path $sidecarDir | Out-Null
$sidecarExe = Join-Path $sidecarDir "carsinos-gateway-$TargetTriple.exe"
Copy-Item -LiteralPath $gatewayExe -Destination $sidecarExe -Force

Push-Location $MissionControlDir
try {
  if (-not $SkipNpmCi) {
    & npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed." }
  }
  Write-Host "Building CarsinOS Mission Control MSI..."
  & npm run tauri -- build --config src-tauri/tauri.beta.conf.json --bundles msi
  if ($LASTEXITCODE -ne 0) { throw "Tauri MSI build failed." }
} finally {
  Pop-Location
}

$msiDir = Join-Path $CargoTargetDir "release\bundle\msi"
$msis = @(Get-ChildItem -LiteralPath $msiDir -Filter "*.msi" -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
if ($msis.Count -ne 1) { throw "Expected exactly one MSI in $msiDir, found $($msis.Count)." }

if (Test-Path -LiteralPath $OutputDir) { Remove-Item -LiteralPath $OutputDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$artifactName = "CarsinOS-Mission-Control-$Version-windows-x64.msi"
$artifactPath = Join-Path $OutputDir $artifactName
Copy-Item -LiteralPath $msis[0].FullName -Destination $artifactPath

$commit = (& git -C $RepoRoot rev-parse HEAD).Trim()
$artifactHash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = [ordered]@{
  schema = "carsinos.release-manifest.v1"
  product = "CarsinOS Mission Control"
  version = $Version
  commit = $commit
  target = $TargetTriple
  created_at_utc = [DateTime]::UtcNow.ToString("o")
  signing = [ordered]@{
    authenticode = "unsigned-beta"
    integrity = "sha256"
    note = "This beta is checksum-verified but not Authenticode-signed. Windows may show an Unknown Publisher warning."
  }
  artifacts = @(
    [ordered]@{
      file = $artifactName
      size_bytes = [int64](Get-Item -LiteralPath $artifactPath).Length
      sha256 = $artifactHash
    }
  )
}
$manifestPath = Join-Path $OutputDir "release-manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
$manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
@(
  "$artifactHash  $artifactName",
  "$manifestHash  release-manifest.json"
) | Set-Content -LiteralPath (Join-Path $OutputDir "SHA256SUMS.txt") -Encoding ASCII

Write-Host "Windows beta package complete: $OutputDir"
Write-Host "$artifactHash  $artifactName"
