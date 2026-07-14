# Public v0.1.0-beta release checklist

This evidence checklist records the live beta release. A row is closed only
with dated command output, an artifact, or live external verification.

## Published beta contract

- [x] Windows x64 MSI is checksum-verifiable and unsigned; UAC/reputation
  warnings are disclosed.
- [x] Gateway sidecar is loopback-only; no remote/public-hosting support.
- [x] Durable local state survives uninstall.
- [x] Backup/verify/restore instructions exclude secrets and require credential
  re-entry after restore.
- [x] No auto-updater.
- [x] Seven-day channel soak explicitly excluded from this beta gate.

## Publication evidence

- [x] Final artifact filename, size, SHA-256, manifest, build command, and
  commit recorded.
- [x] Downloaded MSI re-verified against the matching `SHA256SUMS.txt`.
- [x] Clean Windows x64 lifecycle test passed against the final MSI: install,
  bundled loopback sidecar launch, clean shutdown, uninstall, state survival.
- [x] Backup/verify/restore test evidence retained for final state tooling;
  secrets excluded and credential re-entry confirmed.
- [x] Root build-only Wayland/`quick-xml` exception, nested Tauri audit, and
  informational audit debt reviewed and classified.
- [x] Live GitHub workflow results, repository settings, and reporting-channel
  availability verified; no local-file inference.
- [x] GitHub Release created with MSI, `SHA256SUMS.txt`, manifest, and release
  notes; downloaded release asset hash re-verified.
- [x] Explicit release-owner acceptance of unsigned MSI, no updater, and
  local-only/no-remote boundary.

## Status

**Published and independently verified as a GitHub prerelease on 2026-07-14.**

## Verification record (2026-07-14 UTC)

- `gh api repos/EmergentKnowledgeGroup/CarsinOS --jq .visibility` returned
  `public`; private vulnerability reporting, secret scanning, and push
  protection were then enabled and re-read through the GitHub API.
- `LICENSE` is tracked at final tag commit `3aaecb6`, declares MIT, and matches
  the root Cargo workspace `license = "MIT"` metadata.
- Local pre-PR evidence is retained in the `WINDOWS_PUBLIC_BETA_RELEASE WORK`
  checkpoint: full frontend/visual/security gates, both lockfile audits, state
  recovery roundtrip, MSI hash, and Windows lifecycle passed.
- Release PR [#94](https://github.com/EmergentKnowledgeGroup/CarsinOS/pull/94)
  and Gate 0 follow-up PR
  [#95](https://github.com/EmergentKnowledgeGroup/CarsinOS/pull/95) passed the
  required Mission Control and Security checks, CodeRabbit review, and a
  zero-open-thread audit before merge.
- Live evidence passed on protected `main`: [nightly security](https://github.com/EmergentKnowledgeGroup/CarsinOS/actions/runs/29374084138),
  [retention proof](https://github.com/EmergentKnowledgeGroup/CarsinOS/actions/runs/29374086430),
  [Security Gate 0](https://github.com/EmergentKnowledgeGroup/CarsinOS/actions/runs/29375512432),
  and the [manual Windows package/lifecycle proof](https://github.com/EmergentKnowledgeGroup/CarsinOS/actions/runs/29374087606).
- The [tag-driven publication run](https://github.com/EmergentKnowledgeGroup/CarsinOS/actions/runs/29376047842)
  passed and created the public
  [v0.1.0-beta prerelease](https://github.com/EmergentKnowledgeGroup/CarsinOS/releases/tag/v0.1.0-beta).
- Final MSI: `CarsinOS-Mission-Control-v0.1.0-beta-windows-x64.msi`,
  13,139,968 bytes, SHA-256
  `b125cb12ce6d082a1e96e6d66bc5acdc0c6b0b87ebcde14bd96648420ae4ae2e`.
  The manifest records commit `3aaecb623c84d8340f2fe0a4ed8b98d668982e20`.
- All three checksum-manifest entries and all four GitHub asset digests were
  re-verified after download. The downloaded MSI then passed install, bundled
  gateway launch, state initialization, clean exit, uninstall, and state
  preservation; the operator's original state was restored hash-identically.
- Repository checks verified live: public visibility, private vulnerability
  reporting, secret scanning, push protection, and strict protected-main CI.
- The release owner explicitly authorized publication with the disclosed
  unsigned, no-updater, local-only/no-remote boundary on 2026-07-14.
