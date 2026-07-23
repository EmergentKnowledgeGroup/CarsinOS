# ExecAss EA-401..EA-408 Runtime-Host Gap Map

**Audit date:** 2026-07-22 (America/Chicago)
**Authority checked:** `Z:\carsinos`, branch `codex/execass-backend-runtime-correction`, HEAD `7de545e538b47be57c3d0511634d9d93afb67e06`
**Scope:** live source, dirty working tree, local process and Task Scheduler state. No code, locked specification, checklist, or checkpoint was changed. Tests were not run by this audit.

## Decision

**Windows runtime-host milestone: NOT READY.** Every EA-401..EA-408 item is binary **NOT READY**. EA-401 through EA-403 have useful storage/protocol foundations, and the old Tauri/MSI sidecar path has historical Windows lifecycle evidence, but neither is the required one-host Windows platform implementation. The locked blockerboard independently keeps EA-B09 OPEN (`docs/EXECASS_BACKEND_RUNTIME_PRODUCT_CORRECTION_BLOCKERBOARD.md:22`).

This is not an absence-of-code result. The repository has two materially different states:

1. the new ExecAss persistence/recorder building blocks in the current dirty tree; and
2. an older app-owned `carsinos-gateway` sidecar/package lifecycle.

They are not integrated into one ownership, handoff, Task Scheduler, and exact-artifact proof story.

## Evidence classes used below

| Class | Meaning |
| --- | --- |
| Implemented source | Present in the current checkout, but not necessarily platform-proven. |
| Test-only | A fixture, direct DB setup, fake provider, or test transport proves a narrow property only. |
| Historical/stale | Earlier beta/release evidence for a different lifecycle; cannot satisfy this milestone. |
| Real-platform proven | Exercised on the required OS/profile using the exact hashed candidate. None found for EA-401..EA-408. |

## Clause map and binary readiness

| ID | Required acceptance | Live truth | Verdict |
| --- | --- | --- | --- |
| EA-401 | Gateway evolves into the one runtime host; no wrapper/sibling; source/runtime inventory proves it. | The gateway activates a durable host record only when its scheduler file lock is enabled (`crates/carsinos-gateway/src/main.rs:3989-4024`). But the binary remains named `carsinos-gateway`, the Tauri app always spawns it as an owned sidecar (`apps/mission-control/src-tauri/src/lib.rs:115-140`), and independent legacy launch paths remain. No no-duplicate inventory/assertion exists. | **NOT READY** |
| EA-402 | `(OS user, canonical root, installation)` ownership, OS lock, authenticated native local control, persisted monotonic generation, every-write fencing; dual-process/stale/port/alias proof. | Tuple fields and monotonic records exist (`crates/carsinos-storage/src/execass/runtime_host.rs:25-131`; `migrations/0007_execass_replacement.sql:1800-1827`). Current process exclusion is a state-root-relative `fs2` file lock (`crates/carsinos-gateway/src/main.rs:616-685`), not a tuple-derived OS named lock. Native local signing exists for owner mutations, but host control remains authenticated HTTP (`crates/carsinos-gateway/src/execass_http.rs:1704-1918`). Generation is carried by recorder/effect paths, but it is not universally required at every mutating gateway write. No required process tests exist. | **NOT READY** |
| EA-403 | Desired/actual host state machine and invalid mode combinations. | DTO/storage enums include the complete named state vocabulary (`crates/carsinos-storage/src/execass/types.rs:362-413`), and `start_at_login && app_bound` is rejected (`crates/carsinos-storage/src/execass/runtime_settings.rs:175-182`; HTTP test at `crates/carsinos-gateway/src/main.rs:48083-48104`). Runtime projection can only derive `stopped`, `running_app_bound`, or `running_background` (`runtime_settings.rs:315-325`); no transition engine persists `starting`, `handoff`, `draining`, or `faulted`; no close/forced-exit attention behavior exists. | **NOT READY** |
| EA-404 | Tauri attach-or-start, handoff, background survival, active-work close confirmation/drain, forced-exit recovery truth; real processes; Tauri may stop only an owned app-bound host. | Tauri unconditionally starts a new sidecar (`apps/mission-control/src-tauri/src/lib.rs:115-140`) and kills its child on every app exit (`:179-191`, `:519-523`). There is no attach negotiation, ownership proof, handoff, background survivor, active-work close confirmation, drain acknowledgment, or forced-exit recovery attention item. The old lifecycle script explicitly requires the port to close with the UI (`scripts/test_windows_beta_lifecycle.ps1:107-120`), the inverse of background survival. | **NOT READY** |
| EA-405 | Current-user Windows Task Scheduler register/repair/remove without ordinary admin; scheduler receipts and negative permission test. | No implementation reference to Windows Task Scheduler APIs, `schtasks`, or `Register-ScheduledTask` exists outside the locked specification/checklist. Live inspection found no matching CarsinOS scheduled task. | **NOT READY** |
| EA-406 | Fence legacy/one-click launch paths from independently mutating production state; scan and collision proof. | The workspace still includes `carsinos-gui` (`Cargo.toml:13-15`), whose optional auto-launch starts `carsinos-gateway` directly (`crates/carsinos-gui/src/main.rs:672-693`, `:739-748`). `scripts/one_click_launch.ps1` starts `cargo run -p carsinos-gateway` with an arbitrary `-StateDir` (`scripts/one_click_launch.ps1:150-153`, `:459-482`); the shell launcher does the same (`scripts/one_click_launch.sh:341-352`). These are not fenced to development-only or attach/control-only behavior. | **NOT READY** |
| EA-407 | Exact-hashed Windows RC clean non-developer profile lifecycle: install, login/reboot, close, sleep/wake, crash, repair, upgrade, disable, uninstall, hashes. | Historical beta script covers per-user install, app launch, sidecar presence, clean UI-close/sidecar-stop, uninstall, and state preservation (`scripts/test_windows_beta_lifecycle.ps1:75-120`). It omits the required host modes, Task Scheduler, login/reboot, sleep/wake, crash/recovery truth, repair, upgrade, disable, clean non-developer profile attestation, and an EA-407 exact candidate evidence record. The package script hashes an **unsigned beta** MSI (`scripts/package_windows_beta.ps1:83-111`; `:92-96`), not a runtime-host candidate. | **NOT READY** |
| EA-408 | Windows milestone full regression/security/package gate, complete evidence index, no open Windows blocker, checkpoint. | Checkpoint says EA-401 is next; it does not claim milestone completion. The locked blockerboard leaves EA-B09 OPEN (`docs/EXECASS_BACKEND_RUNTIME_PRODUCT_CORRECTION_BLOCKERBOARD.md:22`) and there is no EA-401..EA-408 evidence index or gate result. | **NOT READY** |

## Reusable foundations (do not replace them)

### 1. Durable identity, root binding, and fencing material — implemented source

- `activate_runtime_host` checks canonical root identity, records state-root generation, installation identity and OS-user digest, ends the previous generation, then increments generation and fencing token in one immediate SQLite transaction (`crates/carsinos-storage/src/execass/runtime_host.rs:25-131`).
- Schema tables make generation and lease identity immutable and prevent reopening released/ended records (`migrations/0007_execass_replacement.sql:1800-1827`, `:3453-3489`).
- Continuation claims, provider attempts, receipts, and technical resources already carry runtime generation/fencing material (`migrations/0007_execass_replacement.sql:600-626`, `:882-918`, `:1433-1459`).
- Existing narrow tests prove monotonic takeover and wrong-root rejection, but not OS/process behavior (`crates/carsinos-storage/src/execass/runtime_host_tests.rs:7-72`).

**Qualification:** this is persistence and effect-path fencing, not the required every-mutation host fence. In particular, the receipt guard verifies that a matching generation/lease row exists, but does not require it to remain unreleased/active (`migrations/0007_execass_replacement.sql:1454-1459`). That is a P1 correctness seam before claiming stale-host write prevention.

### 2. Authenticated local owner proof — implemented source, wrong control-plane layer

- Tauri stores a local-owner secret and uses it to sign exact owner mutation payloads; this is an appropriate reusable authentication primitive (`apps/mission-control/src-tauri/src/lib.rs:23-50`, `:255-384`).
- Gateway configuration requests require authenticated, signed local owner mutation binding (`crates/carsinos-gateway/src/execass_http.rs:1769-1918`).

**Gap:** this reaches the gateway through HTTP and configures desired mode; it is not an authenticated native local host-control channel that can attach, request handoff, prove PID ownership, or request a bounded stop.

### 3. Desired-mode settings — implemented source, incomplete state machine

- Desired mode and `start_at_login` are durable, signed, idempotent settings revisions with exact owner provenance and receipts (`crates/carsinos-storage/src/execass/runtime_settings.rs:39-135`, `migrations/0007_execass_replacement.sql:1943-1951`, `:3533-3553`).
- The gateway exposes a typed read/update route and safe diagnostic fields (`crates/carsinos-gateway/src/main.rs:11019-11022`; `crates/carsinos-gateway/src/execass_http.rs:1704-1766`).

**Gap:** the status fields are largely a projection of “live lease exists” and desired mode. `process_id` and `restart_reason` are always `None` and health is hard-coded `authoritative` (`execass_http.rs:1748-1766`, duplicated at `:1946-1965`). There is no lifecycle controller that can honestly produce the other required states.

### 4. Effect recorder — implemented and real-process-tested, not provisioned/packaged

- `carsinos-effect-recorder` is a workspace binary with owner-only Windows named-pipe DACL plus peer SID verification (`crates/carsinos-effect-recorder/Cargo.toml:48-50`; `src/ipc/windows.rs:49-81`, `:92-147`).
- Its real-process tests exercise restart/query, eight concurrent clients, crash boundaries, root alias rejection, and hostile requests (`crates/carsinos-effect-recorder/tests/real_process_execute_once.rs:19-127`, `:130-180`, `:359-422`; `real_process_crash_matrix.rs:44-187`, `:192-260`). These are valuable **test-only** proof for the recorder contract.
- The hostile EA-312 review explicitly says recorder-sidecar provisioning/startup and install-relative packaging remain EA-401 work (`docs/EXECASS_EA312_FINAL_HOSTILE_REVIEW.md:52-59`). Runtime startup deliberately accepts the provisioning gap and only logs it (`crates/carsinos-gateway/src/main.rs:4025-4040`).

**Gap:** `scripts/package_windows_beta.ps1` builds/packages only `carsinos-gateway` (`:50-58`), and Tauri external binaries list only that gateway (`apps/mission-control/src-tauri/tauri.beta.conf.json:2-14`). The recorder is neither installed nor supervised as a release component.

## Duplicate-host / wrapper / launcher assessment

**Assessment: FAIL — more than one independent production-capable gateway launch path remains.** There is no second ExecAss scheduler engine in the new storage code, but there are multiple ways to create a gateway that can initialize and mutate an arbitrary state root:

| Path | Current behavior | Why it blocks EA-401/EA-406 |
| --- | --- | --- |
| Packaged Tauri | Spawns a `carsinos-gateway` child and owns/kills it. | It is an app-owned wrapper/sidecar, not attach-or-start control of one host. |
| One-click PowerShell | Runs `cargo run -p carsinos-gateway` with caller-selected state directory. | Can make another mutable runtime rather than attaching/fencing. |
| One-click shell | Same direct `cargo run` behavior. | Same collision and duplicate-host risk. |
| Legacy `carsinos-gui` | Auto-launches gateway when an env switch is enabled. | A sibling GUI can independently create the host. |
| Recorder | Separate executable with a deliberately bounded effect-recording role. | **Not** a duplicate host engine; retain it, but make it an installed/supervised companion with its existing authenticated pipe boundary. |

The current scheduler file lock prevents two cooperating gateway processes only when they use the same filesystem state root. It does not establish ownership from OS user + canonical root + installation, does not distinguish app-bound/background, and does not fence aliases/cross-launcher behavior before the process begins ordinary gateway writes (`crates/carsinos-gateway/src/main.rs:616-685`). Port availability is likewise only observed by the old launchers and must not become ownership proof.

## Severity-ranked gaps

### P0 — EA-404: Tauri currently kills the host on UI exit

`apps/mission-control/src-tauri/src/lib.rs:519-523` calls `stop_gateway_sidecar` for every exit, and `:179-191` kills the recorded child. This makes app-bound shutdown the only implemented desktop lifecycle. There is no background survival, handoff, close confirmation, drain, or forced-exit recovery truth.

### P0 — EA-405: no current-user Task Scheduler implementation

No Task Scheduler source was found; no CarsinOS Task Scheduler entry was present in the local OS at audit time. This blocks opt-in background and start-at-login behavior completely.

### P0 — EA-406: independent launchers remain unfenced

`crates/carsinos-gui/src/main.rs:672-748` and `scripts/one_click_launch.ps1:459-482` can independently launch `carsinos-gateway`; the shell launcher duplicates that behavior. They must become attach/control clients or be explicitly development-only and prevented from opening a production root.

### P0 — EA-401 / EA-402: process ownership is only a shared-root file lock

`crates/carsinos-gateway/src/main.rs:616-685` uses `scheduler.instance.lock`; its identity is PID/hostname metadata, while the durable tuple lives separately in SQLite. No OS-named, tuple-scoped lock, native host control channel, alias/port-hijack defense, or two-process assertion ties them together.

### P1 — EA-402: stale generation is not visibly enforced at every write

The generation/lease records and effect/receipt bindings are good foundations, but the host lease is created after generic `carsinos_storage::init` and ordinary gateway state is available (`crates/carsinos-gateway/src/main.rs:3989-4024`). The receipt guard only tests for a matching historical lease tuple (`migrations/0007_execass_replacement.sql:1454-1459`). Add a shared host-fence context required by every mutation boundary and assert an active, unreleased current generation transactionally.

### P1 — EA-403: state enum is ahead of the lifecycle implementation

All required enum values are declared (`crates/carsinos-storage/src/execass/types.rs:391-413`), but projection only emits three effective states (`runtime_settings.rs:315-325`). There are no valid transition guards, durable fault/pause attention records, or active-work drain protocol.

### P1 — EA-401 / recorder provisioning: release cannot execute its provided effect contract out of the box

Gateway startup deliberately treats missing recorder provisioning as expected (`crates/carsinos-gateway/src/main.rs:4029-4040`), while the release package only carries the gateway (`scripts/package_windows_beta.ps1:50-58`; `tauri.beta.conf.json:4`). Package the recorder and add supervisor/health/repair behavior before calling the host usable.

### P2 — EA-407: historical MSI evidence is incompatible with the new requirement

The old lifecycle proof expects sidecar shutdown after UI close (`scripts/test_windows_beta_lifecycle.ps1:107-120`). Its MSI manifest labels the artifact `unsigned-beta` (`scripts/package_windows_beta.ps1:92-96`). Retain it only as historical package evidence; do not reuse it as runtime-host release proof.

## Smallest safe implementation order

1. **Establish the one host authority first (EA-401/EA-402).** Keep `carsinos-gateway` as the executable implementation, but give the release-facing role a single `carsinos-runtime-host` contract and move ownership into one supervisor. Derive a Windows named mutex/lock identity from OS SID + canonical root identity + installation/profile identity; acquire it before any mutable runtime activation. Keep the existing SQLite monotonic lease as the durable fence, but require active-generation fencing at every mutation seam.
2. **Add the native local control protocol before desktop wiring.** Reuse the Tauri local-owner proof and recorder named-pipe patterns, but make a new owner-only local host-control channel exposing only `status`, `attach`, `request_handoff`, `request_stop`, and diagnostics. HTTP/port discovery can report availability but cannot decide ownership.
3. **Make the durable state machine real (EA-403).** Implement transition guards and durable attention for forced exit/restart, then enforce `start_at_login => background` at both native control and storage.
4. **Refactor Tauri to attach-or-start (EA-404).** It must attach to a verified host when available, spawn only if it owns the app-bound start, and only stop the exact app-owned app-bound host after successful drain/confirmation. A background host must survive UI close.
5. **Implement current-user Scheduler register/repair/remove (EA-405).** Bind task action/path/arguments to the installed exact runtime-host binary, retain signed receipts/diagnostics, and prove lack of administrator requirement plus invalid/foreign task rejection.
6. **Fence all legacy entry points (EA-406).** One-click and legacy GUI may attach/control a host or must refuse production roots; they must never independently mutate a release root.
7. **Package host and recorder together (EA-401/EA-407).** Build both exact binaries, record source revision/hashes, install relative to the application, and supervise recorder provisioning. Do not treat the current gateway-only external binary declaration as sufficient.
8. **Run the exact Windows candidate matrix and then EA-408.** Only after the platform receipts and negative cases are complete should the full regression/package/security evidence index be created and EA-B09 be closed.

## Exact first code/test seam

Start at **`crates/carsinos-gateway/src/main.rs:616-685`**, replacing `acquire_scheduler_instance_lock` with a `RuntimeHostOwnership` acquisition boundary whose inputs are the already-available canonical root/installation/SID identity and whose output is the active durable lease plus an OS-owned guard. Wire it before `carsinos_storage::init`-reachable mutable runtime loops at `main.rs:3989-4024`.

First acceptance test should be a new real-process test alongside the existing harness in **`crates/carsinos-gateway/tests/e2e_process.rs`**:

1. create one canonical fresh root and launch host A;
2. launch host B using the same tuple and require attach/conflict with zero scheduler/mutation ownership;
3. kill A, start B, require generation/fence advance;
4. attempt a stale A mutation and require an active-fence rejection;
5. retry through a lexical state-root alias and a port-hijack listener, proving neither can create a second writer.

That test is the smallest credible gate because it exercises the exact existing process-harness family rather than an in-memory lock or direct-DB fixture. Once it passes, extend `crates/carsinos-storage/src/execass/runtime_host_tests.rs` with active-lease and every-write fence assertions, then proceed to the state machine and Tauri work.

## Exact next acceptance tests by checklist item

| ID | Required next proof |
| --- | --- |
| EA-401 | Repository/process inventory asserts only the host binary owns production scheduling; legacy launcher scan fails if direct production-root launch survives. |
| EA-402 | Two-process same tuple; stale owner after kill/takeover; canonical alias; foreign installation; port hijack; mutation fence for receipt, continuation claim, effect dispatch, settings, and control writes. |
| EA-403 | Exhaustive legal/illegal state transitions; `start_at_login` invalid outside background; active-work close requires confirmed pause/stop and completed drain; forced kill yields durable attention on restart. |
| EA-404 | Real Tauri+host processes: attach existing; start absent host; handoff app-bound to background; background survives close; only owned app-bound host stops; forced exit truth is displayed. |
| EA-405 | Actual current-user task create, idempotent repair, disable, delete; inspect task action/user/logon settings; negative no-admin/foreign-task/path-tamper cases; receipt capture. |
| EA-406 | One-click PowerShell/shell and legacy GUI attempt against release root: attach/control or deny without spawning a second mutable host; collision test covers each launcher. |
| EA-407 | Exact candidate hash manifest plus clean non-developer profile: install, enable/disable, login/reboot, UI close, sleep/wake, crash, repair, upgrade, uninstall, state hashes, and scheduler receipts. |
| EA-408 | Locked mandatory suite list, package/security scans, exact artifact verification, evidence index completeness assertion, and blockerboard/checkpoint update only after all Windows evidence is real-platform PASS. |

## Live host snapshot

At audit time there were no `carsinos`, Mission Control, or effect-recorder processes and no matching CarsinOS Task Scheduler entries. `apps/mission-control/src-tauri/binaries` contained an older `carsinos-gateway-x86_64-pc-windows-msvc.exe`; that artifact is not evidence of the new runtime-host/recorder package contract. No live observation was treated as a platform PASS.
