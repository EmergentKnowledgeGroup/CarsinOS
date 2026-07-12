# Security Policy

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability.

Use GitHub's private vulnerability reporting for this repository. Include the affected version or commit, reproduction steps, impact, and any suggested mitigation. Do not include real secrets, personal data, or third-party credentials.

We will acknowledge a complete report as quickly as practical, validate its impact, coordinate remediation, and publish an advisory when disclosure is safe. We do not promise a bounty unless one was explicitly offered before the report.

## Supported versions

Until the first stable release, security fixes target the latest release and the current `main` branch. Pre-release builds are provided without a long-term support guarantee.

## Deployment boundary

CarsinOS is local-first. Non-loopback gateway binding must remain explicitly enabled and protected by TLS termination, authentication, trusted proxy configuration, rate limits, and operator allowlists. Never publish runtime state directories, logs, databases, tokens, or generated security evidence containing sensitive inputs.

## Audited dependency exceptions

The release gate temporarily ignores `RUSTSEC-2026-0194` and
`RUSTSEC-2026-0195` for `quick-xml`. The affected crate is reachable only
through `wayland-scanner`, a build-time proc macro that reads version-pinned
Wayland protocol XML from dependencies. CarsinOS does not feed it runtime or
user-controlled XML. Remove these exceptions as soon as the Wayland dependency
chain permits `quick-xml >= 0.41.0`.
