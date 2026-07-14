param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("Backup", "Verify", "Restore")]
  [string]$Action,
  [string]$StateDir = "",
  [string]$ArchivePath = "",
  [switch]$Force
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $PSCommandPath
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).ProviderPath
$ArchiveSchema = "carsinos.state-backup.v1"
$ExcludedTopLevel = @(
  "cargo-target", "npm-cache", "tmp", "pids", "logs", "locks",
  "launcher-scripts", "codex-bridge", "codex-bridge-workspaces", "secrets"
)

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Resolve-FullPath([string]$Path, [string]$Base) {
  if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
  return [IO.Path]::GetFullPath((Join-Path $Base $Path))
}

function Get-DefaultStateDir {
  if ($env:CARSINOS_STATE_DIR) { return Resolve-FullPath $env:CARSINOS_STATE_DIR $RepoRoot }
  if ($env:LOCALAPPDATA) {
    return Join-Path $env:LOCALAPPDATA "io.carsinos.missioncontrol\state"
  }
  return Join-Path $RepoRoot "runtime\oneclick-state"
}

function Assert-GatewayStopped([string]$Root) {
  $pidFiles = @(
    (Join-Path $Root "pids\gateway.pid"),
    (Join-Path $Root "gateway.pid")
  )
  foreach ($pidFile in $pidFiles) {
    if (-not (Test-Path -LiteralPath $pidFile)) { continue }
    $raw = (Get-Content -LiteralPath $pidFile -Raw).Trim()
    $parsed = 0
    if ([int]::TryParse($raw, [ref]$parsed) -and (Get-Process -Id $parsed -ErrorAction SilentlyContinue)) {
      throw "CarsinOS is still running (gateway pid $parsed). Close CarsinOS before backup or restore."
    }
  }

  $database = Join-Path $Root "carsinos.db"
  if (Test-Path -LiteralPath $database) {
    try {
      $stream = [IO.File]::Open($database, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
      $stream.Dispose()
    } catch {
      throw "CarsinOS state is still in use. Close CarsinOS before backup or restore. $($_.Exception.Message)"
    }
  }
}

function Test-IncludedRelativePath([string]$RelativePath) {
  $normalized = $RelativePath.Replace("\", "/").TrimStart("/")
  if (-not $normalized) { return $false }
  $first = $normalized.Split("/")[0]
  return -not ($ExcludedTopLevel -contains $first)
}

function Get-BackupFiles([string]$Root) {
  @(Get-ChildItem -LiteralPath $Root -Recurse -Force -File | Where-Object {
    $relative = $_.FullName.Substring($Root.TrimEnd("\").Length).TrimStart("\")
    (Test-IncludedRelativePath $relative) -and
      $_.Name -notin @("carsinos.db-wal", "carsinos.db-shm")
  })
}

function Write-BackupManifest([string]$StageRoot, [string]$SourceRoot) {
  $records = @()
  foreach ($file in Get-BackupFiles $StageRoot) {
    if ($file.Name -eq "backup-manifest.json") { continue }
    $relative = $file.FullName.Substring($StageRoot.TrimEnd("\").Length).TrimStart("\").Replace("\", "/")
    $records += [ordered]@{
      path = $relative
      size_bytes = [int64]$file.Length
      sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  }
  $manifest = [ordered]@{
    schema = $ArchiveSchema
    product = "CarsinOS"
    product_version = "0.1.0-beta"
    created_at_utc = [DateTime]::UtcNow.ToString("o")
    source_state_dir = $SourceRoot
    secrets_included = $false
    excluded_top_level = $ExcludedTopLevel
    files = $records
  }
  $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $StageRoot "backup-manifest.json") -Encoding UTF8
}

function Expand-VerifiedArchive([string]$Archive, [string]$Destination) {
  $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
  try {
    foreach ($entry in $zip.Entries) {
      $relative = $entry.FullName.Replace("\", "/")
      if (-not $relative -or $relative.StartsWith("/") -or $relative -match '^[A-Za-z]:' -or $relative.Split("/") -contains "..") {
        throw "Unsafe archive entry: $relative"
      }
      $target = [IO.Path]::GetFullPath((Join-Path $Destination $relative.Replace("/", "\")))
      $prefix = [IO.Path]::GetFullPath($Destination).TrimEnd("\") + "\"
      if (-not $target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Archive entry escapes restore directory: $relative"
      }
      if ($relative.EndsWith("/")) {
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        continue
      }
      New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
    }
  } finally {
    $zip.Dispose()
  }
}

function Test-ExpandedBackup([string]$Root) {
  $manifestPath = Join-Path $Root "backup-manifest.json"
  if (-not (Test-Path -LiteralPath $manifestPath)) { throw "Backup manifest is missing." }
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.schema -ne $ArchiveSchema) { throw "Unsupported backup schema: $($manifest.schema)" }
  if ($manifest.secrets_included -ne $false) { throw "Portable backups must not contain secrets." }
  foreach ($record in @($manifest.files)) {
    $relative = [string]$record.path
    if (-not (Test-IncludedRelativePath $relative)) { throw "Manifest contains excluded path: $relative" }
    $path = [IO.Path]::GetFullPath((Join-Path $Root $relative.Replace("/", "\")))
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd("\") + "\"
    if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $path -PathType Leaf)) {
      throw "Manifest file is missing or unsafe: $relative"
    }
    $file = Get-Item -LiteralPath $path
    if ([int64]$file.Length -ne [int64]$record.size_bytes) { throw "Backup size mismatch: $relative" }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne ([string]$record.sha256).ToLowerInvariant()) { throw "Backup checksum mismatch: $relative" }
  }
  return $manifest
}

function Invoke-Verify([string]$Archive) {
  if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) { throw "Backup archive not found: $Archive" }
  $verifyRoot = Join-Path (Split-Path -Parent $Archive) (".verify-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $verifyRoot | Out-Null
  try {
    Expand-VerifiedArchive $Archive $verifyRoot
    $manifest = Test-ExpandedBackup $verifyRoot
    Write-Host "Backup verified: $Archive ($(@($manifest.files).Count) files)"
  } finally {
    Remove-Item -LiteralPath $verifyRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if (-not $StateDir) { $StateDir = Get-DefaultStateDir }
$StateDir = Resolve-FullPath $StateDir $RepoRoot

if ($Action -eq "Backup") {
  if (-not (Test-Path -LiteralPath $StateDir -PathType Container)) { throw "State directory not found: $StateDir" }
  Assert-GatewayStopped $StateDir
  $backupDir = Join-Path $RepoRoot "runtime\backups"
  New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
  if (-not $ArchivePath) {
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ")
    $ArchivePath = Join-Path $backupDir "carsinos-state-$stamp.zip"
  }
  $ArchivePath = Resolve-FullPath $ArchivePath $RepoRoot
  if (Test-Path -LiteralPath $ArchivePath) { throw "Backup archive already exists: $ArchivePath" }
  $stage = Join-Path (Split-Path -Parent $ArchivePath) (".backup-stage-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $stage | Out-Null
  try {
    foreach ($file in Get-BackupFiles $StateDir) {
      $relative = $file.FullName.Substring($StateDir.TrimEnd("\").Length).TrimStart("\")
      $target = Join-Path $stage $relative
      New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
      Copy-Item -LiteralPath $file.FullName -Destination $target
    }
    Write-BackupManifest $stage $StateDir
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $ArchivePath, [IO.Compression.CompressionLevel]::Optimal, $false)
  } finally {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
  }
  Invoke-Verify $ArchivePath
  $archiveHash = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
  Write-Host "Backup complete: $ArchivePath"
  Write-Host "SHA256: $archiveHash"
  exit 0
}

if (-not $ArchivePath) { throw "-ArchivePath is required for $Action." }
$ArchivePath = Resolve-FullPath $ArchivePath $RepoRoot

if ($Action -eq "Verify") {
  Invoke-Verify $ArchivePath
  exit 0
}

Assert-GatewayStopped $StateDir
$parent = Split-Path -Parent $StateDir
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$stage = Join-Path $parent (".restore-stage-" + [guid]::NewGuid().ToString("N"))
$rollback = Join-Path $parent ((Split-Path -Leaf $StateDir) + ".pre-restore." + [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ"))
New-Item -ItemType Directory -Force -Path $stage | Out-Null
try {
  Expand-VerifiedArchive $ArchivePath $stage
  $manifest = Test-ExpandedBackup $stage
  if (Test-Path -LiteralPath $StateDir) {
    $hasContent = @(Get-ChildItem -LiteralPath $StateDir -Force).Count -gt 0
    if ($hasContent -and -not $Force) { throw "State directory is not empty. Re-run with -Force to preserve it as a rollback copy before restore." }
    if ($hasContent) { Move-Item -LiteralPath $StateDir -Destination $rollback }
    else { Remove-Item -LiteralPath $StateDir -Force }
  }
  Move-Item -LiteralPath $stage -Destination $StateDir
  Write-Host "Restore complete: $StateDir ($(@($manifest.files).Count) files)"
  if (Test-Path -LiteralPath $rollback) { Write-Host "Previous state preserved: $rollback" }
  Write-Host "Provider credentials and gateway tokens were intentionally not restored. Reconnect them in Mission Control."
} catch {
  if ((Test-Path -LiteralPath $rollback) -and -not (Test-Path -LiteralPath $StateDir)) {
    Move-Item -LiteralPath $rollback -Destination $StateDir -ErrorAction SilentlyContinue
  }
  throw
} finally {
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
