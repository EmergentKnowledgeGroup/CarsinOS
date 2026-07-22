# EA-406 legacy launch fencing

## Decision

**SOURCE FENCING READY; PLATFORM MILESTONE NOT READY.**

The maintained one-click scripts now refuse the canonical Mission Control production state root and label every gateway child they create as a development launch. The old macOS GUI package helper is fail-closed unless a developer explicitly supplies a separate state root. This is a source-level fence only: it is not an EA-407 packaged lifecycle claim and no macOS package was built or run.

The exact production identity used by the Windows fence is:

```text
%LOCALAPPDATA%\io.carsinos.missioncontrol\state
```

The Unix fence uses the Tauri-compatible per-user data root for the host OS:

```text
macOS: $HOME/Library/Application Support/io.carsinos.missioncontrol/state
Linux: ${XDG_DATA_HOME:-$HOME/.local/share}/io.carsinos.missioncontrol/state
```

## Launcher and mutation-path inventory

| Path | Can start gateway or mutate canonical state? | EA-406 disposition | Evidence |
| --- | --- | --- | --- |
| `scripts/one_click_launch.ps1` | Yes; ran `cargo run -p carsinos-gateway` with caller-controlled `-StateDir`. | **READY — fenced.** The resolved state root is compared to the canonical production root before any directory/process action. `-ValidateOnly` makes the collision test deterministic; spawned children receive `CARSINOS_LEGACY_LAUNCH_PROFILE=development`. | `scripts/one_click_launch.ps1` `Assert-DevelopmentStateRoot`; `Start-Gateway` child environment; `scripts/test_legacy_launch_fencing.ps1`. |
| `scripts/one_click_launch.cmd` | Yes, through the PowerShell launcher. | **READY — inherited fence.** It only delegates to `one_click_launch.ps1`; it has no independent state/gateway launch. | `scripts/one_click_launch.cmd:13`. |
| `scripts/one_click_launch.sh` | Yes; ran `cargo run -p carsinos-gateway` with `CARSINOS_STATE_DIR`. | **READY — fenced in source.** It resolves the candidate and OS production roots with Python, rejects equality before process reclamation, and exports the development profile for the gateway child. | `scripts/one_click_launch.sh` `assert_development_state_root` and `start_gateway_process`. |
| `scripts/one_click_launch.command` | Yes, through the shell launcher. | **READY — inherited fence.** It delegates only to `one_click_launch.sh`. | `scripts/one_click_launch.command:21`. |
| `scripts/package_macos_app.sh` generated `Contents/MacOS/carsinos` helper | Yes; the generated legacy GUI helper previously used port presence and directly started `carsinos-gateway`. | **READY — fenced in source.** Default launch now stops with exit 64. An isolated developer run must explicitly provide `CARSINOS_LEGACY_GUI_DEVELOPMENT_STATE_ROOT`; equality with the canonical macOS root is rejected, and the child receives the development profile. The port-presence ownership shortcut was removed. | `scripts/package_macos_app.sh` generated launcher heredoc. macOS runtime/package proof was not run. |
| `crates/carsinos-gui/src/main.rs` | Yes; optional GUI auto-launch constructs and spawns `carsinos-gateway` without a state-root/profile fence. | **NOT READY — parent-owned residual EA406-R1.** This Rust legacy-GUI path is outside this lane's allowed files. It must be changed to attach/control or use an explicit isolated development state root before the overall checkbox can close. | `GuiApp::refresh_gateway_state` and `GuiApp::launch_gateway_process` (around lines 667-755). |
| `apps/mission-control/src-tauri/src/lib.rs` | Yes in a release build; it attaches or spawns the packaged sidecar at Mission Control's app-local `state` root. | **NOT READY — parent-owned residual EA406-R1.** This is the active packaged runtime path, not a legacy script. Its attach-or-start/handoff lifecycle belongs to EA-404 and was intentionally not edited here. | `start_gateway_sidecar` (around lines 115-188). |
| `scripts/package_windows_beta.ps1` plus Tauri sidecar config | Packages a gateway but does not launch one itself. | **READY for EA-406 inventory only.** No independent runtime mutation occurs in the package builder. Exact release candidate proof remains EA-407. | `scripts/package_windows_beta.ps1:61-76`; `apps/mission-control/src-tauri/tauri.beta.conf.json`. |
| `scripts/test_windows_beta_lifecycle.ps1` and `.github/workflows/windows-beta-release.yml` | Yes; the lifecycle harness opens the packaged app, which initializes the canonical current-user state root. | **NOT READY — parent-owned residual EA406-R1.** This is an intentional packaged lifecycle harness and cannot be recast as a legacy developer launcher without EA-407 clean-profile authority and test redesign. It was not run or changed. | `scripts/test_windows_beta_lifecycle.ps1:12, 101-103`; workflow invokes it. |
| Gateway process/benchmark tests | Yes, but only under `tempfile::TempDir` roots passed explicitly to `GatewayProcess::spawn`. | **READY — non-production test isolation.** They do not select the canonical Mission Control state root. | `crates/carsinos-gateway/tests/e2e_process.rs:82`; `benchmark_process.rs:19`; `tests/common/mod.rs:43-60`. |
| `crates/carsinos-cli/src/main.rs` | No gateway start command; it prints config or invokes the macOS packaging script. | **READY — no runtime launcher.** Its package command inherits the macOS helper fence above. | `crates/carsinos-cli/src/main.rs`. |
| CI quality/test workflows | Run tests/package scripts, not a separate gateway production launcher. | **READY for direct launch inventory.** The Windows beta release workflow reaches the parent-owned packaged lifecycle residual above. | `.github/workflows/windows-beta-release.yml:86,137`. |

## Collision proof run

Command:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test_legacy_launch_fencing.ps1
```

Result: **PASS**. The test parses the PowerShell launcher, invokes its validation path against the canonical Windows root and requires rejection, then invokes the same path with `runtime\ea406-development-state` and requires acceptance. It also checks that the shell and macOS-source fences are present and that the old macOS port-presence authority check is absent.

Additional source checks:

```powershell
bash.exe -n scripts/one_click_launch.sh
bash.exe -n scripts/package_macos_app.sh
git diff --check -- scripts/one_click_launch.ps1 scripts/one_click_launch.sh scripts/package_macos_app.sh scripts/test_legacy_launch_fencing.ps1 docs/EXECASS_EA406_LEGACY_LAUNCH_FENCING.md
```

The two Bash syntax checks pass on this Windows host. A local `bash.exe scripts/one_click_launch.sh --validate-only` intentionally exits nonzero on `MSYS_NT-*`: the shell fence fails closed because only macOS and Linux production-root semantics are supported by that script. No macOS runtime test was claimed.

## Remaining collision

**EA406-R1 (parent-owned):** `carsinos-gui`, the released Tauri runtime, and the Windows packaged lifecycle harness still provide code paths to the canonical production tuple. The Rust/Tauri sources and EA-407 test harness are explicitly outside this script-only lane. Do not mark EA-406 complete until the parent chooses and implements the attach/control or packaged-runtime treatment for that unified residual.
