# EA-312 Final Hostile Review

## Decision: ACCEPT

The corrected fixed exact-overwrite implementation and composed proof satisfy the reviewed EA-312 hostile acceptance bar. The fixture now observes the actual test transport attached to the production `RecorderClient` and backed by the real `RecorderService`, captures requests before service handling, and asserts after both scheduler calls that exactly one recorder request occurred and that it was `ExecuteOnce`.

The prior exactly-once proof gap is closed: a second physical `ExecuteOnce` or any replay-time `QueryOnly` would add a second ledger entry and fail the exact-length assertion.

## Severity-ranked findings

### Resolved — The composed proof now detects every physical recorder request

Evidence:

- `crates/carsinos-gateway/src/main.rs:47683-47702` constructs the production `RecorderService`, wraps it in `TestRecorderTransport::backed_by_service`, attaches that same transport to the production `RecorderClient`, and retains its shared request ledger in `TestContext`.
- `crates/carsinos-effect-recorder/src/ipc/mod.rs:136-168` shows that the sealed test transport records every signed client request into the shared ledger before passing that same request to the real recorder service.
- `crates/carsinos-effect-recorder/src/ipc/mod.rs:99-107` shows `RecorderClient::send` performs the production identity validation and request signing before selecting the attached sealed transport.
- `crates/carsinos-gateway/src/execass_reference_fixture_tests.rs:288-293` performs the canonical scheduler execution and then its replay.
- `crates/carsinos-gateway/src/execass_reference_fixture_tests.rs:294-312` reads the retained ledger only after both calls, requires exactly one request total, and requires that sole request to be `ExecuteOnce`.

This correction is decision-grade. A second physical `ExecuteOnce` would produce a second captured request. An `AlreadyInvoking -> QueryOnly` replay would likewise produce a second captured request. Either case makes `recorder_requests.len() == 1` fail. The sole-request variant assertion additionally prevents a query-only-only false positive.

No open severity-ranked finding remains in this bounded re-audit.

## Criteria review

1. **Exactly one consequence confirmation; unchanged affirmative replay does not push back again — PASS.** The fixture observes one pending dangerous decision (`execass_reference_fixture_tests.rs:163-180`), posts the identical signed affirmative twice, and requires the same continuation (`:214-244`). It later requires zero pending danger decisions (`:473-476`).

2. **Immutable operand / manifest / effect / attempt binding — PASS.** Recorder material copies the persisted claim, continuation, action, logical-effect, manifest, payload, attempt, fencing, and provider identities (`execass_confirmation_runtime.rs:532-580`), then rechecks the entire material against the current storage attempt immediately before begin (`:600-631`, `:1793-1807`). Storage-derived execution material must also match the prepared effect, payload, and reconciliation key (`:1734-1768`).

3. **`prepared -> invoking` is the sole authorization; no premature production mutation — PASS.** Storage revalidates live claim and objective state before atomically changing the logical effect and provider attempt (`crates/carsinos-storage/src/execass/effect.rs:147-204`). Gateway sends recorder IPC only after that begin result (`execass_confirmation_runtime.rs:1793-1811`). Unsupported/unavailable material settles through the closed adapter-unavailable path without crossing begin (`crates/carsinos-gateway/src/main.rs:35855-35871`).

4. **`ExecuteOnce` only on `Began`; `QueryOnly` after possible invocation/restart — PASS.** The exact mapping is explicit (`execass_confirmation_runtime.rs:634-656`). The scheduler dispatch calls storage begin before selecting the request (`:1793-1811`). Focused tests cover one `ExecuteOnce`, one `QueryOnly`, and restart replay (`:4476-4511`, `:5178-5382`).

5. **No-follow, identity, preimage, and durability boundaries — PASS for the claimed single-file boundary, with residuals below.** The leaf reopens no-follow, rejects non-files/reparse points, checks stable file identity, hashes the open handle, writes/truncates, calls `sync_all`, then rehashes (`crates/carsinos-effect-recorder/src/exact_overwrite.rs:147-185`, `:321-331`, `:392-473`). Windows uses `FILE_FLAG_OPEN_REPARSE_POINT`, write-through, and exclusive sharing (`:415-426`); Unix uses `O_NOFOLLOW` and an exclusive advisory lock (`:429-441`).

6. **Signed recorder evidence and technical-actual convergence — PASS.** Recorder invocation is journal-admitted before provider I/O and terminal evidence is recorded afterward (`crates/carsinos-effect-recorder/src/service.rs:352-424`). Gateway accepts only terminal `Present`/`Absent`/`Unknown`, verifies recorder evidence, and routes it through storage convergence (`execass_confirmation_runtime.rs:1814-1957`). The composed fixture requires terminal continuation and signed `Present` convergence (`execass_reference_fixture_tests.rs:296-315`, `:444-467`).

7. **Unsupported tuples never dispatch — PASS.** The fixed executor rejects unsupported adapter material before journal admission (`crates/carsinos-effect-recorder/src/service.rs:352-355`; `crates/carsinos-effect-recorder/src/executor.rs:43-71`). Gateway constructs installed material only for the exact provider/version/adapter descriptor and otherwise returns unavailable or errors closed (`execass_confirmation_runtime.rs:1723-1769`).

8. **Composed test changes a real disposable file and detects double execution — PASS.** Physical replacement remains real (`execass_reference_fixture_tests.rs:130-161`, `:288-293`, `:315-327`). The shared transport ledger is asserted after execution and replay to contain exactly one `ExecuteOnce` and no second request of any kind (`:294-312`).

9. **No money/purpose/category policing — PASS for inspected EA-312 route.** The ordinary matrix explicitly admits formerly suspect categories without decisions (`execass_reference_fixture_tests.rs:58-128`). The exact dangerous path is selected from the concrete immutable operation and consequence contract; no purpose, category, or monetary-value veto is present in the reviewed dispatch chain.

## Tests and evidence inspected

- Live branch/head and dirty-tree status; current runtime/checkpoint ledgers.
- Live source and working-tree diffs for the gateway confirmation/scheduler route, storage confirmation/claim/effect/recorder boundaries, recorder protocol/service/executor/exact-overwrite implementation, and EA-312 composed fixture.
- Existing focused test source for begin replay, restart recovery, unsupported adapter rejection, exact target mutation, malformed binding rejection, no-follow handling, composed lifecycle convergence, and the corrected shared recorder-request ledger.
- Parent-reported focused evidence: corrected composed fixture PASS 1/1 in 13.43s. This result was not independently rerun in this bounded follow-up; the corrected assertion and transport wiring were independently source-inspected.

## Residual boundary claims that must remain explicit

- The production gateway can activate an already provisioned authenticated recorder client, but recorder-sidecar provisioning/startup and install-relative packaging remain EA-401 work (`crates/carsinos-gateway/src/main.rs:4025-4040`; `execass_confirmation_runtime.rs:1960-1962`). Do not claim packaged out-of-box dangerous execution from EA-312 alone.
- Durability is limited to the already-existing opened file: the implementation performs an in-place write/truncate plus file `sync_all`; it does not provide atomic rename semantics or directory-entry durability. A crash may therefore yield `Unknown`, which must be reconciled.
- Unix `flock` is advisory. Safety assumes cooperating writers or filesystem permissions prevent an uncooperative process from mutating the same inode concurrently.
- Stable inode/file-index identity does not establish exclusive path ownership: hard-link aliases to the same inode remain aliases to the same physical object.
- The installed capability is exactly one bounded overwrite leaf. No broader claim should be made for arbitrary filesystem operations, connectors, money movement, shell commands, or other dangerous tuples.
- `Present` alone proves only that the exact replacement bytes are present at observation time under the bound identity. The corrected composed fixture's separate transport ledger supplies the required request-count witness; production correctness continues to rely on the durable storage and recorder-journal exactly-once boundaries rather than test instrumentation.
