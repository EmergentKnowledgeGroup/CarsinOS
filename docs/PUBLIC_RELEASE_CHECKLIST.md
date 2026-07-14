# Public v0.1.0-beta release checklist

This is an evidence checklist, not a statement that a release is live. Close a
row only with dated command output, an artifact, or live external verification.

## Documented RC contract

- [x] Windows x64 MSI is checksum-verifiable and unsigned; UAC/reputation
  warnings are disclosed.
- [x] Gateway sidecar is loopback-only; no remote/public-hosting support.
- [x] Durable local state survives uninstall.
- [x] Backup/verify/restore instructions exclude secrets and require credential
  re-entry after restore.
- [x] No auto-updater.
- [x] Seven-day channel soak explicitly excluded from this beta gate.

## Evidence needed before publication

- [ ] Final artifact filename, size, SHA-256, manifest, build command, and
  commit recorded.
- [ ] Downloaded MSI re-verified against the matching `SHA256SUMS.txt`.
- [ ] Clean Windows x64 lifecycle test passed against the final MSI: install,
  bundled loopback sidecar launch, clean shutdown, uninstall, state survival.
- [ ] Backup/verify/restore test evidence retained for final state tooling;
  secrets excluded and credential re-entry confirmed.
- [ ] Root build-only Wayland/`quick-xml` exception, nested Tauri audit, and
  informational audit debt reviewed and classified.
- [ ] Live GitHub workflow results, repository settings, and reporting-channel
  availability verified; no local-file inference.
- [ ] GitHub Release created with MSI, `SHA256SUMS.txt`, manifest, and release
  notes; downloaded release asset hash re-verified.
- [ ] Explicit release-owner acceptance of unsigned MSI, no updater, and
  local-only/no-remote boundary.

## Status

**RC documentation complete; public-release evidence incomplete. Do not label
v0.1.0-beta publicly released until every unchecked row is closed.**
