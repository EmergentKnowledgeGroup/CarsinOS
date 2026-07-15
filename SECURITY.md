# Security Policy

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability.

Use [GitHub private vulnerability
reporting](https://github.com/EmergentKnowledgeGroup/CarsinOS/security/advisories/new).
Include the affected version or commit, reproduction steps, impact, and any
suggested mitigation. Do not include real secrets, personal data, or
third-party credentials.

We will acknowledge a complete report as quickly as practical, validate its
impact, coordinate remediation, and publish an advisory when disclosure is
safe. We do not promise a bounty unless one was explicitly offered before the
report.

## Supported versions

Until the first stable release, security fixes target the latest release and the current `main` branch. Pre-release builds are provided without a long-term support guarantee.

## Deployment boundary

CarsinOS `v0.1.0-beta` is local-first. Its packaged Windows gateway is
loopback-only and has no remote/public-hosting support. Do not proxy, tunnel,
or port-forward it. Never publish runtime state directories, logs, databases,
tokens, credential-store exports, or generated security evidence containing
sensitive inputs.

The beta MSI is checksum-verifiable but unsigned. Normal MSI installation
requires administrator/UAC approval and may show a Windows publisher/reputation
warning. Verify the published SHA-256 for the exact RC asset before opening it.
There is no auto-updater.

Portable backup archives intentionally exclude secrets and OS credential-store
material. Re-enter gateway, provider, and channel credentials after restoring.

## Audited dependency exceptions

The root workspace may temporarily ignore the Wayland/`quick-xml` advisories
only for the build-only dependency path. This is not a runtime waiver. The
nested Mission Control Tauri lockfile is upgraded/audited separately, and
informational audit debt must be classified in the release evidence rather than
silently treated as a passing audit.
