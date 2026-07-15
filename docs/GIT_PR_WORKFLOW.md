# CarsinOS Git + PR Workflow

## Goal

Ship work in small PR chunks into `main` so each change set is easy to review,
verify, and roll back.

## Branching

1. Keep `main` stable and deployable.
2. New maintainer work starts from `main` on a `codex/*` branch.
3. One branch equals one focused change set.
4. The local repository checkout is the filesystem source of truth.
5. Do not create git worktrees for normal PR execution. Branch in the local
   repository checkout and work there.

## Local build storage

1. Cargo build artifacts are shared through
   [`.cargo/config.toml`](../.cargo/config.toml).
2. The shared cache lives in the repo-local ignored path
   `.cargo/.shared-cargo-targets/`.
3. This prevents duplicated `target` directories across local runs and nested
   app builds.
4. Do not override the shared target directory unless you are intentionally
   isolating a one-off experiment.

## PR size rules

1. Prefer fewer than 500 net lines per PR unless a larger change is unavoidable.
2. Do not mix concerns.
3. Include tests for behavior changes.

## Required local validation

Run the checks relevant to the change. The full maintainer baseline is:

1. `cargo fmt`
2. Run strict Clippy checks:

   ```bash
   cargo clippy \
     -p carsinos-gateway \
     -p carsinos-storage \
     -p carsinos-protocol \
     -p carsinos-gui \
     -p carsinos-cli \
     --all-targets -- -D warnings
   ```

3. `cargo test`
4. Security scripts when relevant:
   - `scripts/security_pr_gate.sh`
   - `scripts/security_secret_lifecycle_drill.sh`
   - `scripts/security_killswitch_drill.sh`

## PR flow

1. Push the branch to a writable remote: `origin` for maintainers, or the
   contributor's fork remote for external contributors.
2. Maintainers update `runtime/checkpoints/LATEST.md` and
   `runtime/checkpoints/LATEST.json` locally.
3. Open a PR from that branch into this repository's `main`. External
   contributors should select their fork branch as the PR head.
4. Include a short summary and the verification commands that were run.
5. Request CodeRabbit review with `@coderabbitai review`.
6. Run required validation locally before and after review fixes. GitHub's PR
   security and Mission Control quality gates are required on protected `main`;
   local checks remain necessary for fast feedback and platform-specific
   coverage.
7. Address actionable review feedback on the same branch and rerun the relevant
   validation.
8. Limit CodeRabbit follow-up on one PR to two resubmits after the initial
   review request. Do not start a third follow-up cycle on the same PR.
9. Merge to `main` only after required checks, review, and local validation are
   green.

External contributors do not need the maintainer-only ignored checkpoint files.
See [`CONTRIBUTING.md`](../CONTRIBUTING.md) for the public contribution path.

## Why this helps

1. Faster, cleaner reviews.
2. Lower merge risk.
3. Easier rollback.
