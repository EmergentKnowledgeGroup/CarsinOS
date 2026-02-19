# OpenClaw vs carsinOS Gap Audit (2026-02-19)

## Bottom line (plain English)
- carsinOS is no longer "barebones." It now has real session/run APIs, approvals, auth profile management, OpenAI OAuth + Anthropic setup-token flows, local memory search, and Numquam integration wiring.
- It is **not yet OpenClaw-equivalent end to end**. The biggest remaining gaps are channel runtime depth, automation depth, and plugin/skills extensibility.

## What carsinOS already covers well
- Core gateway lifecycle (`health/status/metrics`, sessions, messages, runs, resume).
- Tool execution + approval gating for risky tools (`exec`, `fs_write`, process terminate).
- Auth profile registry, kill-switch concepts, provider profile ordering.
- OpenAI + Anthropic provider calls with retry/fallback across auth profiles.
- Local note memory + retrieval and Numquam context/writeback integration path.

## Major gaps still open

### 1) Real channel operations are still missing
- carsinOS currently has Telegram/Discord routing helpers and config storage, but not live bot/webhook/polling runtimes.
- OpenClaw has production channel runtimes (send/receive/monitor/probe/webhook/audit) and many supported channels.
- Business impact: carsinOS cannot yet function as a true multi-channel assistant runtime.

### 2) Scheduler exists, but job execution is still a stub
- carsinOS scheduler APIs and lease logic are present, but job payload execution currently returns synthetic JSON (`noop/fail`) instead of running agent tasks and delivering results.
- OpenClaw cron runs real agent turns, supports delivery targets, and supports `at/every/cron` schedules.
- Business impact: "automation" exists structurally, but not as real autonomous work.

### 3) No plugin/skills ecosystem yet
- carsinOS has no plugin loader, plugin hooks, plugin tool registration, or skill loading framework.
- OpenClaw has a full plugin runtime + skill ecosystem with install/discovery/hook surfaces.
- Business impact: feature velocity and integration flexibility are bottlenecked.

### 4) Tool surface is much narrower
- carsinOS tools are a small fixed set in one crate.
- OpenClaw has broad tool categories (browser, channel actions, sessions/subagents, nodes/canvas/voice integrations, etc.) plus plugin tools.
- Business impact: less operational leverage per run.

### 5) Provider/model ecosystem is narrower
- carsinOS currently supports `mock/openai/anthropic`.
- OpenClaw supports a broader provider matrix and richer model/auth workflows.
- Business impact: fewer reliability fallback options and less model-market flexibility.

### 6) Operator/ops surface is thinner
- carsinOS CLI is minimal (print config + package).
- OpenClaw has deeper CLI/onboarding/doctor/diagnostic surfaces.
- Business impact: higher operational friction during rollout and troubleshooting.

## Recommended implementation order (clean + pragmatic)
1. Build real Telegram/Discord runtime adapters first (inbound + outbound + status/probe).
2. Upgrade scheduler execution from payload stub to "run real agent task" with delivery policy.
3. Add minimal plugin runtime (tools + hooks + providers), then skills loader.
4. Expand tools needed for your pipeline system.
5. Broaden provider/model support only after channel + automation paths are stable.

## Evidence pointers
- carsinOS channels: `crates/carsinos-channels-telegram/src/lib.rs`, `crates/carsinos-channels-discord/src/lib.rs`
- carsinOS channel config only: `crates/carsinos-gateway/src/main.rs`
- carsinOS scheduler payload stub: `crates/carsinos-gateway/src/main.rs` (`execute_job_payload`)
- OpenClaw cron depth: `src/cron/types.ts`, `src/cron/isolated-agent/run.ts`
- OpenClaw channel runtime depth: `src/telegram/index.ts`, `src/discord/index.ts`
- OpenClaw plugin runtime: `src/plugins/loader.ts`, `src/plugins/types.ts`
