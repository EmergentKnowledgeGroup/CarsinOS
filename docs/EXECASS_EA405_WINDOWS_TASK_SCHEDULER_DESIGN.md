# EA-405 Windows current-user Task Scheduler contract

Status: **implemented and real-current-user lifecycle green; final installed/clean-profile EA-407 evidence remains open**
Scope: Windows 11 23H2+ x86-64, one OS user, one CarsinOS installation/profile, one ExecAss runtime host.

## Locked implementation shape

CarsinOS registers exactly one visible current-user task. The task launches the installed `carsinos-gateway.exe` directly; there is no Mission Control wrapper, shell, service, helper host, or second state root.

| Field | Exact contract |
| --- | --- |
| Task URI | `\CarsinOS ExecAss Runtime` |
| Owner/principal | Current process-token SID; interactive-token logon; LUA/least privilege |
| Trigger | One logon trigger for the same current user |
| Executable | Canonical absolute installed `carsinos-gateway.exe`; no `PATH`, repo target, or arbitrary caller path |
| Arguments | Exactly `--mission-control-runtime-host` |
| Working directory | Canonical parent of the gateway executable |
| Settings | `IgnoreNew`; unlimited task runtime; restart 3 times at one-minute intervals; demand start allowed; no idle, network, battery, or wake prerequisite |
| Management DACL | Protected current-owner and SYSTEM control only; Windows-added duplicate owner read ACE is accepted, but another principal, deny ACE, or weak owner control is a conflict |

The installed gateway flag accepts no additional arguments. It loads the existing gateway token and ExecAss owner secret from the current user Credential Manager, derives the fixed Mission Control state root, binds loopback, and then acquires the normal tuple-bound OS ownership and persisted runtime generation. The Scheduler task is only a launch mechanism; it never grants state authority.

## Reconciliation behavior

The Windows-only gateway adapter uses Task Scheduler 2.0 COM APIs. It never invokes PowerShell, `schtasks.exe`, a Run key, or files under `System32\Tasks` for product mutation.

1. The owner setting commits first. `start_at_login=true` is already invalid outside background mode.
2. Only an installed gateway binary may reconcile the production task. Developer/test binaries do not inspect, alter, or remove it through the HTTP settings path.
3. Missing task creates the exact definition. Exact task is unchanged. Enabled/path drift on the same verified identity is repairable.
4. Foreign owner, action, arguments, principal, trigger, settings, URI, or management ACL fails closed as `identity_conflict`; it is never overwritten or deleted.
5. `start_at_login=false` leaves the exact task disabled during reconciliation. Exact removal first verifies identity and ACL, disables with readback, deletes only that URI, and proves absence. Missing removal is idempotent.
6. Receipts expose only operation, typed outcome, task URI, enabled state, caller-SID digest, and safe failure category. They never expose SID text, XML, SDDL, command line, token, owner secret, state path, or environment.

Windows may normalize a registered SID to a SAM-compatible name in trigger readback and may truncate `IPrincipal::UserId` while preserving the exact full SID in registered XML. The adapter resolves the trigger identity through Windows account lookup and accepts the principal only when the OS-generated `<Principals>` block contains the exact current SID. A SID prefix alone is never accepted as authority.

## Current verification truth

Automated fake-boundary contract tests cover the exact definition, invalid mode combination, secret-free direct action, enabled/disabled behavior, install-path repair, foreign identity/action refusal, weak ACL refusal, malformed readback, permission failure, redacted receipts, and exact removal.

The explicitly ignored real-Windows evidence harness performs:

`absent -> create -> enabled readback -> disable -> disabled readback -> remove -> absent`

That harness is excluded from ordinary regression and requires `CARSINOS_EA405_GATEWAY_EXE` to name a release-shaped absolute gateway path. On July 22, 2026, it passed under a token where `BUILTIN\Administrators` was deny-only, demonstrating that no elevated administrator token was used. The run initially found and then drove fixes for real Windows SID/principal normalization, default task ACL weakness, and a misleading disable receipt label. The exact task was inspected and targeted-cleaned after every failed iteration and was absent after the final passing run.

Current gates:

- Scheduler fake/hostile suite: 11 PASS; real mutation harness ignored by default.
- Real current-user COM lifecycle: 1 PASS.
- Combined gateway/runtime-control/storage strict Clippy: PASS.
- Gateway real-process regression: 22 PASS.

## Remaining EA-407 platform proof

EA-405 source and current-user mechanics are not the full release claim. The following remain open until exercised with the final hashed MSI on a genuinely clean, non-developer standard profile:

- task creation from the installed gateway path rather than the release-shaped evidence binary;
- login and reboot launch, UI attach, background close survival, and app-bound close/drain;
- sleep/wake, crash/retry, occupied-port, state-root-alias, concurrent-launch, and legacy-launch collision cases;
- repair/upgrade path replacement and proof that the old binary never becomes a second writer;
- ARP and silent uninstall removal of the exact task while preserving user state;
- package-linked Scheduler XML/hash, last-result, timestamps, host-generation, state-hash, and screenshot/terminal evidence;
- a durable product receipt for installed scheduler observations if the final evidence reconciliation still requires it beyond the current safe typed runtime receipt.

The current shell cannot create a genuinely clean local profile and must not move or delete the owner's existing CarsinOS state to imitate one. That environmental limitation stays an explicit EA-407 blocker rather than being converted into a false PASS.

## Evidence commands

The product uses COM; these commands are release-evidence inspection only:

```powershell
Get-ScheduledTask -TaskName 'CarsinOS ExecAss Runtime' |
  Format-List TaskName,TaskPath,State,Actions,Triggers,Principal
Get-ScheduledTaskInfo -TaskName 'CarsinOS ExecAss Runtime' |
  Format-List LastRunTime,LastTaskResult,NumberOfMissedRuns
schtasks.exe /query /tn '\CarsinOS ExecAss Runtime' /xml
```

The exact mutation harness is intentionally opt-in:

```powershell
$env:CARSINOS_EA405_GATEWAY_EXE = '<absolute release-shaped carsinos-gateway.exe>'
cargo test -p carsinos-gateway --test windows_task_scheduler `
  real_current_user_scheduler_create_readback_disable_remove --locked -- `
  --ignored --exact --nocapture
```
