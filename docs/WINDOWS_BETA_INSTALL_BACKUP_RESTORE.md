# Windows beta: install, back up, and restore

This guide applies to the `v0.1.0-beta` Windows x64 RC. It is local-only: the
bundled gateway is intended for `127.0.0.1`, not remote or public hosting.

## Verify and install

The release package is `CarsinOS-Mission-Control-v0.1.0-beta-windows-x64.msi`.
Verify the hash in the accompanying `SHA256SUMS.txt` before installing:

```powershell
$Installer = 'C:\Path\To\CarsinOS-Mission-Control-v0.1.0-beta-windows-x64.msi'
$Expected = '<SHA-256 from the matching SHA256SUMS.txt>'
$Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Installer).Hash
if ($Actual -ne $Expected.ToUpperInvariant()) { throw 'MSI checksum mismatch.' }
Start-Process msiexec.exe -Verb RunAs -Wait -ArgumentList @('/i', $Installer)
```

The MSI is unsigned. Normal installation requires administrator/UAC approval
and Windows may show a publisher/reputation warning. Stop if the hash differs
or the prompt is unexpected. There is no auto-updater.

## State location and portable backups

The packaged beta uses:

```text
%LOCALAPPDATA%\io.carsinos.missioncontrol\state
```

It contains durable local state such as the database, attachments, and memory.
It is outside the MSI install directory and survives uninstall. Portable
backups deliberately exclude secrets, logs, locks, temporary files, and build
caches. They do not export Windows Credential Manager/keyring secrets.

Run the supplied helper from a local CarsinOS checkout after closing Mission
Control:

```powershell
$State = Join-Path $env:LOCALAPPDATA 'io.carsinos.missioncontrol\state'
$Archive = Join-Path $env:USERPROFILE 'Documents\carsinos-v0.1.0-beta-state.zip'
.\scripts\carsinos_state.ps1 -Action Backup -StateDir $State -ArchivePath $Archive
.\scripts\carsinos_state.ps1 -Action Verify -ArchivePath $Archive
```

On Windows the helper defaults to the packaged state directory, so `-StateDir`
may be omitted. Supplying it explicitly makes the target obvious before a
backup or restore.

The backup command creates `backup-manifest.json`, verifies every included file
against its SHA-256, and prints the archive SHA-256. The exercised backup test
confirms database, attachments, and memory restore while secrets, logs, and
build cache remain excluded.

## Restore

Close Mission Control first. Verify the archive, then restore it. `-Force`
preserves a non-empty current state directory as a timestamped rollback copy.

```powershell
$State = Join-Path $env:LOCALAPPDATA 'io.carsinos.missioncontrol\state'
$Archive = Join-Path $env:USERPROFILE 'Documents\carsinos-v0.1.0-beta-state.zip'
.\scripts\carsinos_state.ps1 -Action Verify -ArchivePath $Archive
.\scripts\carsinos_state.ps1 -Action Restore -StateDir $State -ArchivePath $Archive -Force
```

Restart Mission Control and re-enter gateway, provider, and channel credentials.
Those secrets are intentionally not restored.

## Uninstall and reinstall

Use Installed apps or the verified MSI:

```powershell
Start-Process msiexec.exe -Verb RunAs -Wait -ArgumentList @('/x', $Installer)
```

Uninstall removes the application but retains the user-owned state directory.
After reinstalling, open Mission Control to use the retained state or restore a
verified portable backup. Do not delete the state directory until you have
confirmed a backup and no longer need the data.
