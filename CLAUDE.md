# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Non-Negotiable SOPs

These procedures are mandatory. Violating any of them requires stopping and correcting before continuing.

### Checkpoint SOP (required)
- Checkpoint files live in `runtime/checkpoints/`.
- Update both `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json` at:
  - phase start
  - post-green tests
  - PR open
  - post-merge
- Include `step`, `note`, `branch`, `head`, `next_cmd`, and validations.
- Resume order after compaction/crash:
  1. read `LATEST.md` + `LATEST.json`
  2. verify `git status --short --branch` and head commit
  3. run recorded `next_cmd`
  4. write fresh checkpoint before edits

### UX Rule (UNSKIPPABLE)
- For any UI/UX implementation, `frontend-design` skill usage is mandatory.
- For multi-phase UX work, checkpoint updates are mandatory at:
  - each phase start
  - post-green validation for each phase (`typecheck`, `lint`, `test`, `build`)
- If either requirement is not met, stop and correct it before continuing.

### Context Compaction SOP (Required for long tasks)
- Treat context compaction as expected, not exceptional.
- Before major edits and after each meaningful milestone, write a checkpoint:
  - `python3 "${CODEX_HOME:-$HOME/.codex}/tools/context_checkpoint.py" --repo-root "<repo_root>" snapshot --step "<step>" --note "<progress>" --next-cmd "<next command>" --label "<tag>"`
- After compaction/interruption, recover state first:
  - `python3 "${CODEX_HOME:-$HOME/.codex}/tools/context_checkpoint.py" --repo-root "<repo_root>" resume --live`
- Resume flow:
  1. Confirm branch/head drift from the checkpoint output.
  2. Open changed-file diffs listed in checkpoint.
  3. Execute the recorded `next_cmd`.
  4. Write a new checkpoint before proceeding to the next block.

## Project Overview

CarsinOS is a Rust-native AI agent orchestration gateway and operations console — a ground-up replacement for OpenClaw. It provides a command & control center for managing autonomous AI agents across multiple communication channels with human oversight, approval gates, and real-time monitoring.

## Repository Structure

**Rust workspace** (15 crates) + **React/Tauri desktop app** (1 app):

- `crates/carsinos-gateway` — Core Axum HTTP/WS server. 100+ API endpoints, scheduler, circuit breakers, job execution, event broadcasting. This is where most backend work happens.
- `crates/carsinos-core` — Domain types, gateway config, plugin/hook/skill registries, channel lifecycle traits, auth profiles.
- `crates/carsinos-protocol` — All HTTP/WS request/response/event types (200+ types). Shared contract between gateway and frontend.
- `crates/carsinos-storage` — SQLite via `rusqlite`. Repositories for all entities, migrations, AppPaths. Tests here validate data layer.
- `crates/carsinos-tools` — Tool implementations (`exec`, `fs.read/write`, `web.fetch/search`, `channel.action`) with sandbox policy enforcement.
- `crates/carsinos-providers` — AI provider abstractions (OpenAI, Anthropic, OpenRouter, Ollama, vLLM, mock).
- `crates/carsinos-channels-*` (7 crates) — Channel adapters: Discord, Telegram, Signal, Slack, WhatsApp, Twitch, BlueBubbles.
- `crates/carsinos-cli` — CLI via `clap`. Gateway startup, macOS app bundling.
- `crates/carsinos-gui` — Legacy egui desktop app (Mission Control is the canonical UI).
- `apps/mission-control/` — React 19 + Vite + Tauri 2 desktop app. Operator console with boards, agent mail, cockpit, events, calendar, focus tabs.

## Build & Development Commands

### Rust (from repo root)

```bash
# Run the gateway
CARSINOS_GATEWAY_TOKEN="change-me" cargo run -p carsinos-gateway

# Test endpoint
curl -H "Authorization: Bearer change-me" http://127.0.0.1:18789/api/v1/health

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p carsinos-gateway
cargo test -p carsinos-storage

# Run a single test by name
cargo test -p carsinos-gateway test_name_here

# Format
cargo fmt

# Lint (all validated crates)
cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings

# Package macOS app
cargo run -p carsinos-cli -- package-macos --release
```

### Frontend — Mission Control (from `apps/mission-control/`)

```bash
npm run dev          # Vite dev server on port 1420
npm run build        # TypeScript check + Vite build
npm run typecheck    # TypeScript only
npm run lint         # ESLint
npm run tauri:dev    # Full Tauri dev (Rust backend + React frontend)
npm run tauri:build  # Production Tauri build
```

### Pre-PR Validation

Run before opening any PR (per `docs/GIT_PR_WORKFLOW.md`):

```bash
cargo fmt
cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings
cargo test
scripts/security_pr_gate.sh  # when security-relevant
```

## Architecture

### Data Flow

```text
Mission Control (React) ──HTTP/WS──▶ Gateway (Axum) ──▶ SQLite
                                         │
                                    ┌────┼────┐
                                    ▼    ▼    ▼
                               Providers Channels Tools
                            (LLM APIs) (Discord, (fs, exec,
                                       Telegram)  web)
```

### Key Architectural Patterns

- **Event-driven UI sync**: Gateway emits `WsEventFrame` via `tokio::sync::broadcast` for every state change. Frontend subscribes to `/ws` and updates reactively — no polling.
- **Per-session serialization**: One active LLM run per session at a time (mutex per session_id). Prevents race conditions in conversations.
- **Circuit breakers**: Track consecutive failures per scope (provider, channel, tool). Auto-open after threshold with cooldown. Stored in `circuit_breaker_states` table.
- **Approval state machine**: `requested → approved/denied` (terminal). Supports resolution from GUI, Discord, or Telegram.
- **Tool sandboxing**: Policy-based allowlisting for filesystem roots, binaries, network hosts. Timeout enforcement and output truncation.
- **Hook/plugin system**: Plugin manifests with exec kind (subprocess/daemon), hook points, dynamic registration, capability declarations.
- **Scheduler**: File-based lock for single-leader semantics, cron expression parsing, job lease tracking per agent.

### Frontend Architecture (Mission Control)

- Controller hooks pattern: `useAppController`, `useRuntimeConnectionController`, `useGatewayEvents`, `useBoardsController`, `useMissionControlController`, `useAgentMailController`, `useCockpitController`
- Feature modules under `src/features/` (agentMail, boards, calendar, cockpit, events, focus)
- Shared UI primitives under `src/ui/` (Surface, Chip, etc.)
- Gateway connection settings persisted to Tauri keychain
- Optimistic UI updates with rollback on API failure

### Database

SQLite with migrations in `/migrations/0001_init.sql`. 50+ tables covering agents, sessions, messages, runs, tool calls, approvals, jobs, boards, agent mail, circuit breakers, security audit events, and embeddings.

## Git Workflow

- Keep `main` stable and deployable
- New work on `codex/*` branches
- Prefer < 500 net LOC per PR, no mixed concerns
- Update `runtime/checkpoints/LATEST.md` and `LATEST.json` before PRing
- CI: `.github/workflows/pr-gate.yml` (security audit) + `.github/workflows/nightly-security.yml`

## Key Environment Variables

- `CARSINOS_GATEWAY_BIND` — Server bind address (default: `127.0.0.1:18789`)
- `CARSINOS_GATEWAY_TOKEN` — Bearer auth token (generated if omitted)
- `CARSINOS_AUTH_MODE` — `static_bearer` or `jwt`
- `CARSINOS_STATE_DIR` — Override state directory
- `CARSINOS_SECRET_STORE` — `keychain` or `memory` (tests use `memory`)
- `CARSINOS_LOG_FORMAT` — `compact`/`text`/`pretty`/`json`
- `CARSINOS_TOOL_ALLOWED_ROOTS` — Filesystem sandbox roots
- `CARSINOS_TOOL_ALLOWED_BINARIES` — Binary exec allowlist

Full list in README.md.

## Frontend Claudit (UX/UI Audit)

A comprehensive frontend audit is tracked in [`frontend_claudit.md`](./frontend_claudit.md). This file contains 131 verified findings (4 retracted) across 21 sections covering 16 frontend features + shared UI primitives + API layer + cross-cutting patterns. All 182 finding IDs verified against source code with line references. Severity: 4 Critical, 33 High, 61 Medium, 33 Low. Reference this file when doing any frontend UI/UX work — it is the canonical list of known gaps, accessibility issues, missing states, and inconsistent patterns.

## Tech Stack

**Backend**: Rust 2021, Axum 0.8, Tokio, rusqlite 0.37, serde, tracing, reqwest, clap 4, jsonwebtoken, keyring
**Frontend**: React 19, TypeScript 5.9, Vite 7, Tauri 2, @dnd-kit, @tanstack/react-virtual
**CI**: GitHub Actions (cargo-audit, security scripts)
