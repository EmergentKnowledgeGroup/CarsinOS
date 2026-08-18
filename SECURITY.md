# Security policy

## Report a vulnerability privately

Do not open a public issue for a suspected vulnerability.

Use [GitHub private vulnerability reporting](https://github.com/EmergentKnowledgeGroup/CarsinOS/security/advisories/new).
Include the affected commit, a minimal sanitized reproduction, impact, and any
suggested mitigation. Do not include real secrets, personal data, private
messages, or live runtime databases.

We will acknowledge a complete report, validate the impact, coordinate a fix,
and publish an advisory when disclosure is safe. No bounty is implied unless it
is explicitly offered.

## Supported source

Until a packaged release is published from this new public repository, security
fixes target the current `main` branch. Please report the exact commit tested.

## Operational boundary

CarsinOS is designed for local-first operation. Treat a source instance as
local unless you have independently reviewed the network, TLS, authentication,
and reverse-proxy configuration for your environment.

Never publish runtime state directories, logs, databases, token files,
credential-store exports, receipt archives, or generated diagnostic evidence.
For the project’s security design, see [the security model](docs/SECURITY_MODEL.md).
