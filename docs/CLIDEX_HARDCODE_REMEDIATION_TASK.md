# CLIdex Task: Hardcoded Runtime-Value Remediation (MC-CONF-006..009)

## Context
Audit is complete:
- `docs/HARDCODED_RUNTIME_VALUES_AUDIT.md`

Triage tickets are defined in:
- `APPDEX_IMPLEMENTATION_TICKET_PACK.md` (`MC-CONF-006`, `MC-CONF-007`, `MC-CONF-008`, `MC-CONF-009`)

## Objective
Implement the audited hardcoded-value remediations without regressing runtime behavior.

## Required implementation sequence
1. `MC-CONF-006` (global externalization)
   - Externalize:
     - Numquam base URL default (`crates/carsinos-gateway/src/main.rs`)
     - Numquam principal id/name defaults (`crates/carsinos-gateway/src/main.rs`)
     - OpenAI OAuth redirect default (`crates/carsinos-gateway/src/main.rs`)
     - Gateway bind default (`crates/carsinos-core/src/lib.rs`)
     - GUI gateway base URL default (`crates/carsinos-gui/src/main.rs`)
   - Route through runtime config/wizard-backed fields first, env vars second, hardcoded fallback last (or fail-closed for internet mode).

2. `MC-CONF-007` (provider endpoints)
   - Externalize vLLM/Ollama fallback URLs in `crates/carsinos-providers/src/lib.rs`.
   - Resolve from provider profile/runtime config before any literal fallback.

3. `MC-CONF-008` (security policy externalization)
   - Externalize tool network allowlist default from `crates/carsinos-tools/src/lib.rs` into runtime security config.
   - Enforce explicit operator-set allowlist in internet-facing mode.

4. `MC-CONF-009` (channel defaults)
   - Externalize channel default model provider/model values from `crates/carsinos-gateway/src/main.rs`.
   - Replace static channel tool-provider fallback with runtime-derived allowlist from enabled channels.

## Test gates (must be green)
- `cargo fmt --all --check`
- `cargo clippy -p carsinos-gateway -p carsinos-storage -p carsinos-protocol -p carsinos-gui -p carsinos-cli --all-targets -- -D warnings`
- `cargo test --workspace --locked`
- `scripts/security_pr_gate.sh`

## Deliverables
1. Code changes implementing `MC-CONF-006..009`.
2. Updated docs where config contract keys changed.
3. Updated tests for new config precedence/validation behavior.
4. Checkpoint updates in `CHECKPOINT.md` and `runtime/checkpoints/LATEST.{md,json}`.

## Non-negotiables
- No plaintext secrets in config payloads.
- No source hardcoded deployment IDs/domains/endpoints in operational paths that are now covered by config.
- Any intentionally retained constants must be added to `docs/security/HARDCODED_VALUE_ALLOWLIST.csv` with owner + expiry.
