# CarsinOS public v0.1.0-beta blockerboard

| ID | State | Evidence required before public release |
|---|---|---|
| PUB-001 | CLOSED | The GitHub repository is already public; publication now means creating and verifying the beta Release, not changing repository visibility. |
| PUB-002 | CLOSED | MIT license exists and matches workspace metadata. |
| PUB-003 | READY | `SECURITY.md` exists; private reporting channel/settings still require live verification before release messaging claims them. |
| PRB-01 | READY | RC docs cover unsigned checksum-verified MSI, UAC, loopback sidecar, state survival, non-secret recovery, no updater, and no remote hosting. |
| PRB-02 | OPEN | Final MSI, commit/build provenance, `release-manifest.json`, and `SHA256SUMS.txt` recorded. |
| PRB-03 | OPEN | Clean Windows install/launch/loopback/shutdown/uninstall/state-preservation evidence from the final MSI. |
| PRB-04 | OPEN | Final backup/verify/restore evidence confirms portable state excludes secrets and credential re-entry works. |
| PRB-05 | OPEN | Audit evidence retains the scoped root build-only Wayland/`quick-xml` exception, nested Tauri audit, and informational-debt classification. |
| SEC-001 | OPEN | Dependency and security gate evidence attached to the RC; no blanket advisory waiver. |
| CI-001 | OPEN | Current GitHub workflow runs and repository/release settings checked live. Local workflow files alone are insufficient. |
| QA-001 | OPEN | Fresh Windows desktop proof for the final package retained. |
| PRB-06 | OPEN | GitHub Release created, MSI plus hash/manifest uploaded, and downloaded asset hash re-verified. |
| PRB-07 | OPEN | Explicit acceptance of unsigned distribution and no-auto-update/no-remote-support scope. |
| SOAK-001 | EXCLUDED | Seven-day channel soak is outside this local desktop beta's acceptance gate; do not mark it complete here. |

**NO-GO:** public release is blocked while any
OPEN release-critical row remains. This board does not assert a live GitHub
release, workflow, setting, or public host.
