# CarsinOS Git + PR Workflow

## Goal
Ship work in small PR chunks into `main` so CodeRabbit can review each change set.

## Branching
1. Keep `main` stable and deployable.
2. New work always starts from `main` on a `codex/*` branch.
3. One branch = one focused change set.
4. The local checkout at `/Users/domusanimae/Documents/openclaw replacement/carsinos` is the filesystem source of truth.
5. Do not create git worktrees for normal PR execution. Branch in the source-of-truth checkout and work there.

## Local Build Storage
1. Cargo build artifacts are shared through [`.cargo/config.toml`](/Users/domusanimae/Documents/openclaw replacement/carsinos/.cargo/config.toml).
2. This prevents duplicated `target` directories across local runs and nested app builds.
3. Do not override the shared target dir unless you are intentionally isolating a one-off experiment.

## PR Size Rules
1. Prefer < 500 net LOC per PR unless a larger change is unavoidable.
2. No mixed concerns in one PR.
3. Include tests for behavior changes.

## Required Local Validation
1. `cargo fmt`
2. `cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings`
3. `cargo test`
4. Security scripts when relevant:
   - `scripts/security_pr_gate.sh`
   - `scripts/security_secret_lifecycle_drill.sh`
   - `scripts/security_killswitch_drill.sh`

## PR Flow
1. Push branch to origin.
2. Open PR into `main`.
3. Update `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json`.
4. Let CodeRabbit + CI run.
5. Address review comments on same branch.
6. Merge to `main` when green.

## Why This Helps
1. Faster, cleaner reviews.
2. Lower merge risk.
3. Easier rollback if a regression appears.
