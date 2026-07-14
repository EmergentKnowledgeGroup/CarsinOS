# CarsinOS public v0.1.0-beta blockerboard

| ID | State | Retained release evidence |
|---|---|---|
| PUB-001 | CLOSED | The GitHub repository is already public; see the dated [verification record](PUBLIC_RELEASE_CHECKLIST.md#verification-record-2026-07-14-utc). |
| PUB-002 | CLOSED | MIT license exists and matches workspace metadata; see the dated [verification record](PUBLIC_RELEASE_CHECKLIST.md#verification-record-2026-07-14-utc). |
| PUB-003 | CLOSED | `SECURITY.md`, private vulnerability reporting, secret scanning, and push protection verified live on 2026-07-14. |
| PRB-01 | CLOSED | Public docs disclose unsigned checksum-verified MSI, UAC, loopback sidecar, state survival, non-secret recovery, no updater, no remote hosting, and desktop-only layout support. |
| PRB-02 | CLOSED | Final manifest records commit `3aaecb6`; MSI size 13,139,968 and SHA-256 `b125cb12ce6d082a1e96e6d66bc5acdc0c6b0b87ebcde14bd96648420ae4ae2e`. |
| PRB-03 | CLOSED | Tag workflow `29376047842` and a separate downloaded-public-MSI replay passed install/launch/loopback/shutdown/uninstall/state preservation. |
| PRB-04 | CLOSED | Final happy-path plus checksum/size/traversal tests passed; portable backup excludes secrets and docs require credential re-entry. |
| PRB-05 | CLOSED | Root/nested audits have zero unignored vulnerabilities; target/test/build-only informational debt is classified in the release notes. |
| SEC-001 | CLOSED | PR security gates, nightly `29374084138`, Gate 0 `29375512432`, and retention `29374086430` passed without a blanket advisory waiver. |
| CI-001 | CLOSED | Protected-main settings, live workflow runs, public repository, reporting, scanning, and push-protection settings verified through GitHub APIs. |
| QA-001 | CLOSED | Final downloaded Windows MSI lifecycle and preserved-state hash comparison passed. |
| PRB-06 | CLOSED | [GitHub prerelease](https://github.com/EmergentKnowledgeGroup/CarsinOS/releases/tag/v0.1.0-beta) is live with MSI, checksums, manifest, and SBOM; all downloaded digests passed. |
| PRB-07 | CLOSED | Release owner explicitly authorized the disclosed unsigned, no-auto-update, local-only/no-remote boundary on 2026-07-14. |
| SOAK-001 | EXCLUDED | Seven-day channel soak is outside this local desktop beta's acceptance gate; do not mark it complete here. |

**GO:** every in-scope release-critical row is closed with retained evidence.
`SOAK-001` remains explicitly excluded, not completed.
