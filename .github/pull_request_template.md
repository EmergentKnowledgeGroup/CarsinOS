## Summary
- What changed:
- Why:

## Scope
- [ ] Small/reviewable chunk (target < 500 net LOC unless justified)
- [ ] No unrelated refactors
- [ ] Backward compatibility impact called out

## Validation
- [ ] `cargo fmt`
- [ ] `cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Relevant drill/benchmark scripts (if security/runtime changes)

## Security
- [ ] No secret material logged or persisted in plaintext
- [ ] Auth/authz implications reviewed
- [ ] Rate-limit/abuse behavior unchanged or explicitly updated
- [ ] Audit coverage present for high-risk mutation paths

## Checkpoint SOP
- [ ] `runtime/checkpoints/LATEST.md` and `runtime/checkpoints/LATEST.json` updated at phase start and post-green
- [ ] `CHECKPOINT.md` entry added for this PR chunk
