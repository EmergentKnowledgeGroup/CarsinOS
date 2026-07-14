# CarsinOS

Local-first AI operations for running assistants, approvals, schedules, channels, memory, and tools from one auditable control plane.

> **v0.1.0-beta is live:** Windows x64 only, checksum-verifiable but unsigned,
> and local-only. Do not expose the bundled loopback gateway publicly.

## Windows public beta

The Windows x64 MSI bundles Mission Control and a gateway that listens on
`127.0.0.1`. It needs normal MSI administrator/UAC approval and may show an
unsigned-publisher/reputation warning. Download the MSI and
`SHA256SUMS.txt` from the same
[v0.1.0-beta prerelease](https://github.com/EmergentKnowledgeGroup/CarsinOS/releases/tag/v0.1.0-beta).
Verify the SHA-256
before opening it. Copy the expected digest from the matching
`SHA256SUMS.txt`, then require an exact comparison:

```powershell
$Installer = 'C:\Path\To\CarsinOS-Mission-Control-v0.1.0-beta-windows-x64.msi'
$Expected = '<SHA-256 from the matching SHA256SUMS.txt>'
$Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Installer).Hash
if ($Actual -ne $Expected.ToUpperInvariant()) { throw 'MSI checksum mismatch.' }
```

The beta has no auto-updater and no remote/public-hosting support. Its durable
state is outside the MSI install directory and survives uninstall. Backups are
verified, portable copies of non-secret state only; credentials must be
re-entered after a restore.

Published MSI SHA-256:
`b125cb12ce6d082a1e96e6d66bc5acdc0c6b0b87ebcde14bd96648420ae4ae2e`.
The supported beta experience is the Windows desktop layout; narrow/mobile
Cockpit layouts are not a supported target in this release.

- [Windows install, backup, and restore](docs/WINDOWS_BETA_INSTALL_BACKUP_RESTORE.md)
- [v0.1.0-beta release notes](docs/releases/v0.1.0-beta.md)
- [Public release checklist](docs/PUBLIC_RELEASE_CHECKLIST.md)

## What it includes

- Rust workspace with gateway, GUI, CLI, channels, storage, providers, tools
- Token-authenticated API + WebSocket event stream
- Auth profile controls (OpenAI PKCE, direct Anthropic API keys, and profile ordering)
- Memory notes CRUD + local embeddings retrieval + bounded prompt injection
- SQLite initialization/migrations + structured logging + benchmark suite

## Run (after Rust toolchain install)

```bash
cd "$REPO_DIR" # set to your local carsinos clone path
CARSINOS_GATEWAY_TOKEN="change-me" cargo run -p carsinos-gateway
```

## One-click local launch (gateway + Mission Control)

```bash
cd "$REPO_DIR" # set to your local carsinos clone path
scripts/one_click_launch.sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\one_click_launch.ps1
```

Windows double-click:

```text
scripts\one_click_launch.cmd
```

Notes:
- Script prompts for a gateway token; press Enter to auto-generate one.
- New one-click launches automatically reclaim prior repo-owned Mission Control and gateway processes from the same local checkout before starting.
- Gateway and UI ports auto-shift when busy in `--web` mode.
- PowerShell prompts for Desktop/Web mode by default, and still accepts explicit `-Web` / `-Tauri`, `-GatewayPort`, `-UiPort`, `-Token`, `-StateDir`, and `-CargoTargetDir`.
- Use `scripts/one_click_launch.sh --tauri` for desktop mode (requires free port `1420`).
- Finder double-click launcher: `scripts/one_click_launch.command` (desktop mode by default, with mode prompt).
- Windows double-click launcher: `scripts\one_click_launch.cmd` (desktop mode by default, with mode prompt).
- Logs are written to `runtime/oneclick-state/logs/`.

## Test endpoint

```bash
curl -H "Authorization: Bearer change-me" http://127.0.0.1:18789/api/v1/health
```

## Packaging (macOS)

Bundle a local `.app`:

```bash
cd "$REPO_DIR" # set to your local carsinos clone path
cargo run -p carsinos-cli -- package-macos --release
```

Debug bundle variant:

```bash
cargo run -p carsinos-cli -- package-macos --debug
```

The bundle is created under `target/dist/carsinOS.app` by default. Launcher binary:

`target/dist/carsinOS.app/Contents/MacOS/carsinos`

## Security automation

Please report suspected vulnerabilities privately as described in [`SECURITY.md`](SECURITY.md). CarsinOS is licensed under the [`MIT License`](LICENSE).

Per-PR hard gate run:

```bash
cd "$REPO_DIR" # set to your local carsinos clone path
scripts/security_pr_gate.sh
```

Nightly deep scan run:

```bash
scripts/security_nightly_deep_scan.sh
```

Kill-switch drill run:

```bash
scripts/security_killswitch_drill.sh
```

Secret lifecycle drill run:

```bash
scripts/security_secret_lifecycle_drill.sh
```

Security artifacts are written under `runtime/security/reports/`.

## Git/PR review flow

- Follow `docs/GIT_PR_WORKFLOW.md`.
- Do not treat local workflow files as proof of a live GitHub run, repository
  setting, public release, or hosted asset. Verify those separately before
  publication.
- The repository contains workflow definitions for ad hoc and desktop-release runs:
  - `.github/workflows/pr-gate.yml`
  - `.github/workflows/nightly-security.yml`
  - `.github/workflows/windows-beta-release.yml`

## Secret lifecycle scheduling (jobs)

Secret rotate/revoke operations can run on a cadence through `jobs` payload modes.

Scheduled rotation payload:

```json
{
  "mode": "secret.rotate_profile",
  "auth_profile_id": "<auth_profile_id>",
  "reason": "scheduled cadence rotation"
}
```

Scheduled revoke payload:

```json
{
  "mode": "secret.revoke_profile",
  "auth_profile_id": "<auth_profile_id>",
  "reason": "scheduled revocation",
  "remove_secret": true,
  "disable_profile": true,
  "kill_switch_scope": "profile"
}
```

## Environment

- `CARSINOS_GATEWAY_BIND` (default: `127.0.0.1:18789`)
- `CARSINOS_GATEWAY_TOKEN` (if omitted, generated at startup and logged)
- `CARSINOS_STATE_DIR` (optional override)
- `CARSINOS_AUTH_MODE` (`static_bearer`/`jwt`)
- `CARSINOS_AUTH_JWT_ISSUER`, `CARSINOS_AUTH_JWT_AUDIENCE`, `CARSINOS_AUTH_JWT_HS256_SECRET` (required in `jwt` mode)
- `CARSINOS_AUTH_JWT_REVOKED_JTIS` (comma-separated JTI deny-list)
- `CARSINOS_PUBLIC_BIND_ALLOWED` (`true|false`, required for non-loopback bind)
- `CARSINOS_EDGE_TLS_TERMINATED` (`true|false`, required when public bind is enabled)
- `CARSINOS_TRUST_PROXY_HEADERS` + `CARSINOS_TRUSTED_PROXY_ALLOWLIST` (fail-closed trusted proxy mode)
- `CARSINOS_RATE_LIMIT_ENABLED`, `CARSINOS_RATE_LIMIT_WINDOW_SECONDS`, `CARSINOS_RATE_LIMIT_PER_IP`, `CARSINOS_RATE_LIMIT_PER_PRINCIPAL`, `CARSINOS_RATE_LIMIT_RUN_ENDPOINT`, `CARSINOS_RATE_LIMIT_APPROVAL_ENDPOINT`
- `CARSINOS_OPERATOR_ALLOWLIST` (comma-separated operator IDs required for approval actions)
- `CARSINOS_TOOL_ALLOWED_ROOTS` (comma-separated filesystem roots for `fs.read`/`fs.write` and exec workdirs)
- `CARSINOS_TOOL_ALLOWED_BINARIES` (comma-separated binary allowlist for `tool.exec`)
- `CARSINOS_TOOL_NETWORK_POLICY` (`allowlist`/`deny_all`) + `CARSINOS_TOOL_NETWORK_ALLOWLIST` (host allowlist for web tools)
- `CARSINOS_LOG_FILTER` (default: `info,tower_http=info`)
- `CARSINOS_LOG_FORMAT` (`compact`/`text`/`pretty`/`json`, default: `compact`)
- `CARSINOS_LOG_STDOUT` (`true|false`, default: `true`)
- `CARSINOS_LOG_FILE` (`true|false`, default: `true`; writes rolling logs under `<state>/logs`)
- `CARSINOS_LOG_FILE_PREFIX` (default: `gateway.log`)
- `CARSINOS_SECRET_STORE` (`keychain`/`memory`, default: `keychain`; tests default to memory)
- `CARSINOS_SECRET_SERVICE` (keychain service name, default: `carsinos`)
- `CARSINOS_GUI_AUTO_LAUNCH_GATEWAY` (`true|false`, default: `true`)
- `CARSINOS_LOCAL_MEMORY_ENABLED` (`true|false`, default: `true`)
- `CARSINOS_LOCAL_MEMORY_TOP_K` (default: `4`)
- `CARSINOS_LOCAL_MEMORY_MAX_CANDIDATES` (default: `128`)
- `CARSINOS_LOCAL_MEMORY_MAX_CHARS` (default: `1200`)
