# carsinOS

Ground-up Rust AI gateway inspired by OpenClaw.

## Current scope (Milestone 0)

- Rust workspace with gateway, GUI, CLI, channels, storage, providers, tools
- Token-authenticated API + WebSocket event stream
- OAuth/auth profile controls (OpenAI PKCE + Anthropic setup-token ingest + profile ordering)
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

Notes:
- Script prompts for a gateway token; press Enter to auto-generate one.
- Gateway and UI ports auto-shift when busy in `--web` mode.
- Use `scripts/one_click_launch.sh --tauri` for desktop mode (requires free port `1420`).
- Finder double-click launcher: `scripts/one_click_launch.command` (desktop mode by default, with mode prompt).
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
- CI is wired through:
  - `.github/workflows/pr-gate.yml`
  - `.github/workflows/nightly-security.yml`

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
