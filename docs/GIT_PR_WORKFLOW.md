# CarsinOS Git + PR Workflow

## Goal
Ship work in small PR chunks into `main` so CodeRabbit can review each change set.

## Branching
1. Keep `main` stable and deployable.
2. New work always starts from `main` on a `codex/*` branch.
3. One branch = one focused change set.

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
3. Let CodeRabbit + CI run.
4. Address review comments on same branch.
5. Merge to `main` when green.

## Why This Helps
1. Faster, cleaner reviews.
2. Lower merge risk.
3. Easier rollback if a regression appears.
