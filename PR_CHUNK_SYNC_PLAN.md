# PR Chunk Sync Plan (Local FS -> GitHub)

Date: 2026-03-01
Branch baseline: `codex/mc-onboarding-wizard`
Rule: local filesystem is source of truth; no local data loss.

## Goals
- Ship all current local changes to GitHub in reviewable chunks.
- Preserve dependency order so each chunk is mergeable to `main`.
- Keep regression gates attached to each chunk.

## Frontend Bug-Fix Closure (Claude Pass)
- Real-bug checklist from `docs/MC_UI_REVIEW_AUDIT.md` was rechecked before chunking.
- Status: no unresolved high/real bugs found in the audited set.
- Remaining items are design/polish choices, not release blockers.

## Safety Protocol (No-Loss)
1. Snapshot checkpoint before each chunk stage.
2. Never hard reset or discard working tree.
3. Commit only explicit file sets per chunk.
4. Push branch and open PR.
5. Wait for CodeRabbit review, apply accepted fixes, re-run gates.
6. Merge PR, pull `main`, continue with next chunk.

## Chunk Order

### Chunk 1 - Gateway/Protocol/Storage Core
Purpose: backend contract/runtime changes and DB migration updates.

Files:
- `crates/carsinos-gateway/src/main.rs`
- `crates/carsinos-protocol/src/lib.rs`
- `crates/carsinos-storage/src/lib.rs`
- `migrations/0001_init.sql`

Gates:
- `cargo test -p carsinos-gateway`
- `cargo test -p carsinos-protocol`
- `cargo test -p carsinos-storage`

PR title:
- `core: gateway/protocol/storage runtime and contract upgrades`

### Chunk 2 - Mission Control UI/Core Refactor + Design System
Purpose: main frontend runtime shell/features/layout/controller updates.

Files:
- `apps/mission-control/src/App.tsx`
- `apps/mission-control/src/app/AppContent.tsx`
- `apps/mission-control/src/app/AppShell.tsx`
- `apps/mission-control/src/app/useGatewayEvents.ts`
- `apps/mission-control/src/app/useTheme.ts`
- `apps/mission-control/src/features/boards/BoardLane.tsx`
- `apps/mission-control/src/features/boards/BoardsPage.tsx`
- `apps/mission-control/src/features/boards/useBoardsController.ts`
- `apps/mission-control/src/features/cockpit/CockpitPage.tsx`
- `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx`
- `apps/mission-control/src/features/cockpit/cockpitLayout.ts`
- `apps/mission-control/src/features/cockpit/useCockpitController.ts`
- `apps/mission-control/src/features/cockpit/CockpitCanvas.tsx`
- `apps/mission-control/src/features/cockpit/CockpitEditToolbar.tsx`
- `apps/mission-control/src/features/cockpit/CustomWidgetBuilderModal.tsx`
- `apps/mission-control/src/features/cockpit/CustomWidgetRenderer.tsx`
- `apps/mission-control/src/features/cockpit/WidgetPickerModal.tsx`
- `apps/mission-control/src/features/cockpit/cockpitApiRunner.ts`
- `apps/mission-control/src/features/cockpit/cockpitDataSources.ts`
- `apps/mission-control/src/features/cockpit/useWidgetPagination.ts`
- `apps/mission-control/src/ui/NotificationCenter.tsx`
- `apps/mission-control/src/ui/Toast.tsx`
- `apps/mission-control/src/ui/useToasts.ts`
- `apps/mission-control/src/styles.css`

Gates:
- `cd apps/mission-control && npm run typecheck`
- `cd apps/mission-control && npm run lint`
- `cd apps/mission-control && npm run build`

PR title:
- `mc-ui: shell/cockpit/boards refactor and design-system uplift`

### Chunk 3 - Onboarding + Provider Catalog Option C
Purpose: config-first constants/storage keys and server-driven provider/model catalogs.

Files:
- `apps/mission-control/src/constants.ts`
- `apps/mission-control/src/storageKeys.ts`
- `apps/mission-control/src/lib/api.ts`
- `apps/mission-control/src/lib/runtime.ts`
- `apps/mission-control/src/lib/ws.ts`
- `apps/mission-control/src/lib/providerCatalog.ts`
- `apps/mission-control/src/types.ts`
- `apps/mission-control/src/features/onboarding/OnboardingWizard.tsx`
- `apps/mission-control/src/features/onboarding/onboardingState.ts`
- `apps/mission-control/src/features/onboarding/steps/StepConnect.tsx`
- `apps/mission-control/src/features/onboarding/steps/StepProvider.tsx`
- `apps/mission-control/src/features/onboarding/useOnboardingController.ts`
- `apps/mission-control/src/features/team/TeamPage.tsx`

Gates:
- `cd apps/mission-control && npm run typecheck`
- `cd apps/mission-control && npm run lint`
- `cd apps/mission-control && npm run build`
- `cargo test -p carsinos-gateway provider_models_`

PR title:
- `mc-onboarding: option-c provider/model catalogs and config constants`

### Chunk 4 - Packaging/Tooling/Docs
Purpose: repo hygiene, launcher, docs, and execution metadata.

Files:
- `.gitignore`
- `CHECKLIST.md`
- `apps/mission-control/package.json`
- `apps/mission-control/package-lock.json`
- `apps/mission-control/src-tauri/Cargo.toml`
- `scripts/launch_mission_control_tauri_dev.command`
- `ASSISTANT_TOOL_CONTRACT_V1_SPEC.md`
- `docs/COCKPIT_REDESIGN_SPEC.md`
- `docs/CODEBASE_E2E_AUDIT.md`
- `docs/DELTA_LOG.md`
- `docs/MC_FRONTEND_HARDCODED_CONFIG_CANDIDATES.md`
- `docs/MC_HARDCODED_CONFIG_ACTION_PLAN.md`
- `docs/MC_HARDCODED_CONFIG_OPTION_C_IMPLEMENTATION_SPEC.md`
- `docs/mission-control-designpass.md`

Gates:
- `cargo test --workspace --locked`
- `cd apps/mission-control && npm run build`

PR title:
- `docs+ops: roadmap/checklist/spec and launcher alignment`

## Execution Notes
- Keep PRs sequential (Chunk 1 -> 4) to minimize rebase churn.
- After each merge, sync local branch from `origin/main`.
- If CodeRabbit proposes safe, non-regressive fixes, apply before merge.
- If conflict appears, reconcile on local branch without dropping unstaged files.
