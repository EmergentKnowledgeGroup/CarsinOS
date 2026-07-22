# EA-404 Runtime Forced-Exit Recovery Evidence

Status: source and local process evidence green; packaged clean-profile platform evidence remains open.

## Behavior proved

When a successor gateway finds a live predecessor generation left behind by a forced exit, it atomically:

1. fences and ends the predecessor generation;
2. classifies the interruption from its authoritative prior state;
3. activates the successor generation;
4. snapshots only bounded active-work counts and their binding digest;
5. creates one `runtime_paused` item in the canonical attention authority;
6. creates one `runtime_recovery` receipt in the canonical global receipt journal and anchor; and
7. emits the generation-bound runtime-host outbox event.

The attention and receipt use a tagged `runtime_host` subject. They do not invent a delegation, reuse the hidden global-control carrier as the incident subject, or classify the incident as a recovery choice.

An orderly stopped predecessor creates no incident. Same-host activation replay creates no duplicate. Running, already-faulted, and interrupted-drain predecessors retain distinct safe end reasons. Any failure in the atomic package rolls back the predecessor transition, successor, attention, outbox event, receipt, and anchor together.

## Canonical evidence shape

- Attention kind: `runtime_paused`
- Attention scope: `runtime_host`
- Receipt kind: `runtime_recovery`
- Receipt scope: `runtime_host`
- Receipt subject: `runtime_host_generation`
- Runtime aggregate: `execass-runtime-host`
- Forced-exit predecessor end reason: `gateway_forced_exit_takeover`
- Faulted predecessor end reason: `gateway_fault_takeover`
- Interrupted-drain predecessor end reason: `gateway_drain_interrupted_takeover`

The projection validates the stored authoritative receipt scope, receipt subject and kind, exact predecessor generation, outbox family/aggregate/revision, and predecessor end reason before exposing the item. Hostile tampering fails closed.

## Validation evidence

- Storage deterministic all-feature suite: 404/404 library tests, 2/2 receipt-integrity CLI tests, and doc tests passed.
- Focused runtime-host suite: 11/11 passed, including forced takeover, faulted takeover, interrupted drain, orderly stop, same-host replay, rollback collision, and projection/receipt tamper cases.
- Protocol suite: 42/42 passed; contract generator 5/5 passed; generated-schema drift check and semantic contract validator passed.
- Gateway suite: 408/408 unit tests, 2/2 benchmark-process tests, 22/22 real-process E2E tests, and 11/11 non-mutating scheduler contract tests passed.
- The process E2E forcibly drops the primary gateway, starts a successor, proves generation advancement, reads the versioned summary API, verifies one runtime attention and one runtime receipt with no delegation field, and verifies the predecessor is faulted with exactly one incident package.
- Strict all-target/all-feature Clippy passed for storage, protocol, and gateway. Workspace formatting and `git diff --check` passed; Git emitted line-ending warnings only.

## Final-source Windows release candidate

The exact locally built package containing this recovery source is:

- Version: `v0.1.0-beta-ea407-rc2`
- MSI: `CarsinOS-Mission-Control-v0.1.0-beta-ea407-rc2-windows-x64.msi`
- Size: `18,792,448` bytes
- MSI SHA-256: `5a44cb1bd7ae0b74966f53a1474cb8b3a379e8b0e09f95046d57ed5be20c2c3c`
- Gateway SHA-256: `ba4b1a023f0eed9a637641550939b4f00c81b81efd30aef56b145e9da127416c`
- Effect recorder SHA-256: `af71ba79d28f5144ceed0e59b23d7a2c2c03d95ffb1a05b7fbcd1729b5e11a68`

Independent hash reads match the release manifest and the exact sidecar files consumed by WiX. The WiX source includes both `carsinos-gateway.exe` and `carsinos-effect-recorder.exe` in the package. This is an unsigned local dirty-tree proof build; its manifest commit identifies the branch baseline, not a claim that the uncommitted implementation already exists at that commit.

## Remaining platform boundary

This evidence does not claim the clean Windows 11 23H2+ standard-user install/login/reboot/uninstall matrix or macOS M4 process proof. The current machine has no proven authorized clean Windows 11 guest route, and the existing user profile is intentionally not used as fake clean-profile evidence. Those EA-407/EA-B09 platform gates remain open.
