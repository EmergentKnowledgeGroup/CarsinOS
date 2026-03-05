# Mission Control (CarsinOS)

Mission Control is the CarsinOS operator UI built with React + TypeScript + Vite and packaged with Tauri.

## Scripts

- `npm run dev` - run Vite dev server
- `npm run build` - typecheck + production bundle
- `npm run typecheck` - TypeScript project check
- `npm run lint` - ESLint
- `npm run test:unit` - Vitest unit suite
- `npm run test:e2e:core` - Playwright core suite (onboarding, baseline connect, crash recovery)
- `npm run test:e2e:full` - full Playwright suite
- `npm run test:e2e:tauri-smoke` - desktop smoke automation with step-by-step screenshots and trace artifacts
- `npm run quality:acceptance:p1` - validate Section 7 `[P1]` acceptance matrix coverage
- `npm run quality:acceptance:p2` - validate Section 7 `[P2]` acceptance matrix coverage
- `npm run quality:acceptance:p3` - validate Section 7 `[P3]` acceptance matrix coverage
- `npm run quality:acceptance:p4` - validate Section 7 `[P4]` acceptance matrix coverage
- `npm run quality:gate` - run Mission Control quality gate (`pr` profile)
- `npm run quality:gate:pr` - run Mission Control quality gate (`pr` profile)
- `npm run quality:gate:release` - run Mission Control quality gate (`release` profile)
- `npm run tauri:dev` - run desktop app in Tauri dev mode

Quality gate profiles are config-driven via [`quality-gate.config.json`](./quality-gate.config.json).  
Tauri smoke visual artifacts are written to `runtime/quality-gate/artifacts/tauri-smoke/<timestamp>/` (screenshots, trace, logs, and run manifest).
Phase-scoped acceptance mapping for Section 7 bullets is tracked in:
- [`docs/mission-control_p1_acceptance_matrix.json`](../../docs/mission-control_p1_acceptance_matrix.json)
- [`docs/mission-control_p2_acceptance_matrix.json`](../../docs/mission-control_p2_acceptance_matrix.json)
- [`docs/mission-control_p3_acceptance_matrix.json`](../../docs/mission-control_p3_acceptance_matrix.json)
and validated by `mc-acceptance-p1/p2/p3` in the quality gate.

## Onboarding Wizard

Mission Control includes a first-run onboarding wizard to help new users configure setup without editing files.

### Auto-open behavior

Wizard auto-opens when setup is incomplete, including cases like:
- missing gateway URL
- missing gateway token
- no agents
- no usable provider path configured

Dismiss state is stored locally for 24 hours:
- key: `mission_control.onboarding.dismissed_at_ms`

### Reopen behavior

Users can reopen onboarding at any time using `Setup Wizard` in the top bar.

### V1 provider paths

- Anthropic setup token ingest
- OpenAI OAuth (start + finish)
- Local connector mode (no OAuth)

### Secret handling

- Gateway token and provider secrets are never persisted in browser localStorage by the wizard logic.
- Secrets are sent to gateway/keychain paths and draft inputs are cleared after successful setup actions.
