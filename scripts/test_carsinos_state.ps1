param()

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $PSCommandPath
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).ProviderPath
$Root = Join-Path $RepoRoot ("runtime\state-backup-test-" + [guid]::NewGuid().ToString("N"))
$Source = Join-Path $Root "source"
$Restored = Join-Path $Root "restored"
$Archive = Join-Path $Root "fixture.zip"
$TamperStage = Join-Path $Root "tamper-stage"
$ChecksumTamperedArchive = Join-Path $Root "checksum-tampered.zip"
$SizeTamperedArchive = Join-Path $Root "size-tampered.zip"
$TraversalArchive = Join-Path $Root "traversal.zip"

function Assert([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw "ASSERTION FAILED: $Message" }
}

function Assert-VerifyFails([string]$Candidate, [string]$ExpectedError, [string]$Message) {
  $previousPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  $output = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $ScriptDir "carsinos_state.ps1") -Action Verify -ArchivePath $Candidate 2>&1 | Out-String)
  $exitCode = $LASTEXITCODE
  $ErrorActionPreference = $previousPreference
  Assert ($exitCode -ne 0) $Message
  Assert ($output -match [regex]::Escape($ExpectedError)) "$Message (expected error: $ExpectedError)"
}

try {
  New-Item -ItemType Directory -Force -Path (Join-Path $Source "attachments"), (Join-Path $Source "memory"), (Join-Path $Source "logs"), (Join-Path $Source "secrets"), (Join-Path $Source "cargo-target") | Out-Null
  [IO.File]::WriteAllBytes((Join-Path $Source "carsinos.db"), [Text.Encoding]::UTF8.GetBytes("sqlite-fixture"))
  Set-Content -LiteralPath (Join-Path $Source "attachments\proof.txt") -Value "attachment"
  Set-Content -LiteralPath (Join-Path $Source "memory\memory.md") -Value "remember this"
  Set-Content -LiteralPath (Join-Path $Source "logs\gateway.log") -Value "ephemeral"
  Set-Content -LiteralPath (Join-Path $Source "secrets\provider.key") -Value "must-not-leave"
  Set-Content -LiteralPath (Join-Path $Source "cargo-target\artifact.bin") -Value "generated"

  & (Join-Path $ScriptDir "carsinos_state.ps1") -Action Backup -StateDir $Source -ArchivePath $Archive
  Assert ($LASTEXITCODE -eq 0) "backup command failed"
  & (Join-Path $ScriptDir "carsinos_state.ps1") -Action Verify -ArchivePath $Archive
  Assert ($LASTEXITCODE -eq 0) "verify command failed"
  & (Join-Path $ScriptDir "carsinos_state.ps1") -Action Restore -StateDir $Restored -ArchivePath $Archive
  Assert ($LASTEXITCODE -eq 0) "restore command failed"

  Assert (Test-Path -LiteralPath (Join-Path $Restored "carsinos.db")) "database was not restored"
  Assert (Test-Path -LiteralPath (Join-Path $Restored "attachments\proof.txt")) "attachment was not restored"
  Assert (Test-Path -LiteralPath (Join-Path $Restored "memory\memory.md")) "memory was not restored"
  Assert (-not (Test-Path -LiteralPath (Join-Path $Restored "logs"))) "logs must not be portable"
  Assert (-not (Test-Path -LiteralPath (Join-Path $Restored "secrets"))) "secrets must not be portable"
  Assert (-not (Test-Path -LiteralPath (Join-Path $Restored "cargo-target"))) "build cache must not be portable"

  [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $TamperStage)
  $proofPath = Join-Path $TamperStage "attachments\proof.txt"
  $proofBytes = [IO.File]::ReadAllBytes($proofPath)
  $proofBytes[0] = $proofBytes[0] -bxor 1
  [IO.File]::WriteAllBytes($proofPath, $proofBytes)
  [IO.Compression.ZipFile]::CreateFromDirectory($TamperStage, $ChecksumTamperedArchive)
  Assert-VerifyFails $ChecksumTamperedArchive "Backup checksum mismatch" "same-size checksum tampering must be rejected"

  Set-Content -LiteralPath (Join-Path $TamperStage "attachments\proof.txt") -Value "attachment-with-extra-bytes"
  [IO.Compression.ZipFile]::CreateFromDirectory($TamperStage, $SizeTamperedArchive)
  Assert-VerifyFails $SizeTamperedArchive "Backup size mismatch" "size tampering must be rejected"

  $stream = [IO.File]::Open($TraversalArchive, [IO.FileMode]::CreateNew)
  $zip = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
  try {
    $entry = $zip.CreateEntry("../escape.txt")
    $writer = New-Object IO.StreamWriter($entry.Open())
    try { $writer.Write("must-not-escape") } finally { $writer.Dispose() }
  } finally {
    $zip.Dispose()
    $stream.Dispose()
  }
  Assert-VerifyFails $TraversalArchive "Unsafe archive entry" "archive path traversal must be rejected"
  Assert (-not (Test-Path -LiteralPath (Join-Path $Root "escape.txt"))) "path traversal created a file outside extraction root"

  Write-Host "CarsinOS state backup/verify/restore test passed."
} finally {
  if (Test-Path -LiteralPath $Root) { Remove-Item -LiteralPath $Root -Recurse -Force }
}
