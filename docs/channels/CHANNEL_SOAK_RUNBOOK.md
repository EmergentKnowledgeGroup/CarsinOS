# Channel Soak Runbook

## Purpose
Execute and document the Telegram/Discord 7-day soak required by checklist `O6`:
- reconnect resilience
- retry/degrade behavior
- message roundtrip integrity
- channel approval callback roundtrip

The soak harness is implemented by `scripts/channel_soak_runner.py`.

## Prerequisites
- Running carsinOS gateway reachable at `CARSINOS_BASE_URL`.
- Bearer token with channel adapter/admin permissions (`CARSINOS_AUTH_TOKEN`).
- Runtime channel transport mode configured and healthy for Telegram and Discord.
- Staging identifiers:
  - `CARSINOS_TELEGRAM_CHAT_ID`
  - `CARSINOS_TELEGRAM_USER_ID`
  - `CARSINOS_DISCORD_CHANNEL_ID`
  - `CARSINOS_DISCORD_AUTHOR_ID`

## Quick Smoke (No Sleep)
```bash
python3 scripts/channel_soak_runner.py \
  --base-url "${CARSINOS_BASE_URL}" \
  --token "${CARSINOS_AUTH_TOKEN}" \
  --telegram-chat-id "${CARSINOS_TELEGRAM_CHAT_ID}" \
  --telegram-user-id "${CARSINOS_TELEGRAM_USER_ID}" \
  --discord-channel-id "${CARSINOS_DISCORD_CHANNEL_ID}" \
  --discord-author-id "${CARSINOS_DISCORD_AUTHOR_ID}" \
  --iterations 5 \
  --interval-seconds 5 \
  --no-sleep \
  --label "smoke"
```

## 7-Day Soak Command
```bash
python3 scripts/channel_soak_runner.py \
  --base-url "${CARSINOS_BASE_URL}" \
  --token "${CARSINOS_AUTH_TOKEN}" \
  --telegram-chat-id "${CARSINOS_TELEGRAM_CHAT_ID}" \
  --telegram-user-id "${CARSINOS_TELEGRAM_USER_ID}" \
  --discord-channel-id "${CARSINOS_DISCORD_CHANNEL_ID}" \
  --discord-author-id "${CARSINOS_DISCORD_AUTHOR_ID}" \
  --duration-hours 168 \
  --interval-seconds 300 \
  --min-success-rate 0.99 \
  --max-failure-rate 0.01 \
  --label "soak-7d"
```

## Artifacts
Reports are written to `runtime/channels/reports/`:
- `channel-soak-<timestamp>.log`
- `channel-soak-<timestamp>.json`
- `channel-soak-latest.json` (latest summary pointer)

When executed in GitHub Actions (`.github/workflows/channel-soak.yml`), reports are uploaded as run artifacts.

## Report Fields to Validate
- `status` must be `green`.
- Per-provider `success_rate` and `failure_rate` must satisfy thresholds.
- Per-provider `outbound_reply_status_counts` should show dominant `sent`.
- `approval_roundtrip.status` must be `passed`.
- `runtime_reconnect_delta` and final health are captured per provider.

## Failure Handling
If `status=red`:
1. Review `reasons` in the JSON report.
2. Inspect the matching `.log` file for failing iteration/provider details.
3. Cross-check gateway runtime status (`/api/v1/channels/runtime/status`) and security audit entries (`/api/v1/security/audit`).
4. Re-run with narrower scope (`--skip-telegram` or `--skip-discord`) to isolate failures.

## Ownership Note
Checklist items `R2`, `R3`, `R5`, and `R6` still require operator-owned production values and signoff.  
This runbook and harness remove implementation blockers, but live 7-day signoff still requires those owner inputs.

## GitHub Workflow Slice
Use `.github/workflows/channel-soak.yml` for bounded execution slices with artifact retention.

Notes:
- GitHub-hosted jobs have runtime limits, so the workflow is intended for short soak slices.
- A full 7-day signoff should run via external orchestration (screen/tmux/systemd/runner host) using the 7-day command above.
- Required repository secrets for non-dry-run workflow execution:
  - `CARSINOS_BASE_URL`
  - `CARSINOS_AUTH_TOKEN`
  - `CARSINOS_TELEGRAM_CHAT_ID`
  - `CARSINOS_TELEGRAM_USER_ID`
  - `CARSINOS_DISCORD_CHANNEL_ID`
  - `CARSINOS_DISCORD_AUTHOR_ID`
  - `CARSINOS_OPERATOR_PEER_ID` (optional; defaults in script if unset)
