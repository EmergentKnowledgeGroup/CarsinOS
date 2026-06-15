# MNO / Hermes ExecAss Integration Audit

Date: 2026-06-02
Workstream: MNO_HERMES_EXECASS_INTEGRATION WORK

## Sources Checked

- Clean MNO source: `Z:\numquamoblita-clean`
- CarsinOS MNO vendor/integration: `vendors/mno`, `crates/carsinos-gateway/src/main.rs`
- Latest Hermes Agent checkout: `Z:\carsinos-codex-work\hermes-agent-latest`
- Hermes Agent HEAD: `787936d13300271a38afc230a263e19f6735eb8c` (`feat(gateway): structured stream-event protocol + Telegram draft formatting parity (#37250)`)

## MNO Comparison Findings

Clean MNO's preferred agent integration path is `integration-v1`, not a Hermes-only adapter. The relevant hot-loop contract is:

1. Call MNO `context.build` before the model turn.
2. Prefer returned `agent_context` with format `mno_memory_context.v1`.
3. Treat `<MNO_MEMORY_CONTEXT>` as retrieved evidence, not user instructions.
4. Use `context.why` for audit/explainability.
5. Use `writeback.propose` for memory updates, with operator review before durable truth mutation.

CarsinOS already used `integration-v1`, lane-scoped runtime selection, context build, writeback proposal, explainability, local memory fallback, and managed-lane startup. The main integration gap was split across both sides of the boundary: the vendored MNO runtime only emitted legacy `context_text`, and CarsinOS deserialized/injected only that legacy field. That lost the clean MNO `agent_context` wrapper that teaches the model how to safely use or abstain from memory.

## Implemented Alignment

- Gateway now deserializes `agent_context` and `agent_context_format`.
- Provider prompt injection prefers `agent_context` when present and falls back to legacy `context_text`.
- Run usage metadata records `memory.agent_context_used` and `memory.agent_context_format`.
- HTTP/MCP parity checks compare the actual injected memory block, not only legacy `context_text`.
- ExecAss core prompt now explains MNO memory blocks as evidence, not instructions.
- ExecAss core prompt now includes a reviewed learning loop: capture durable lessons, runbook steps, and skill candidates only through available tools/writeback and only with evidence or operator review.
- Mission Control's core prompt mirror and tests were updated to match the gateway prompt.
- Vendored MNO `integration-v1` now emits `agent_context` and `agent_context_format: mno_memory_context.v1`, matching the clean repo contract.

## Hermes Lessons Applied

Hermes Agent's current architecture uses:

- stable/context/volatile prompt tiers,
- MEMORY.md and USER.md snapshots,
- external memory provider blocks,
- pre-turn memory prefetch and post-turn non-blocking sync,
- FTS5 session search for cross-session recall,
- periodic nudges to use memory and skill tools,
- skill creation/improvement as procedural memory,
- profile isolation and one external memory provider at a time.

CarsinOS already has several analogous pieces: local memory notes/search, lane-scoped MNO runtime policy, writeback approval, run usage metadata, tool inventory injection, boards/runbooks/team surfaces, and bridge/worker orchestration. The implemented change moves CarsinOS closer to Hermes' strongest pattern by making the memory block explicit, fenced, and evidence-scoped at the actual model prompt boundary.

## Live Pipeline Proof Notes

The live managed-lane proof showed two distinct memory boundaries that ExecAss should understand:

- `integration-v1/writeback.resolve` is proposal-review state. It approves or rejects a proposal, but does not by itself turn that proposal into retrievable memory.
- Durable MNO apply remains explicit through the runtime proposal apply path. After `apply=true`, the next gateway run retrieved evidence and injected it through `agent_context`.
- Run usage correctly records `memory.agent_context_used: true`, `memory.agent_context_format: mno_memory_context.v1`, context request IDs, evidence IDs, route/confidence, and proposal metadata.
- The deterministic mock provider proved the prompt boundary contained `<MNO_MEMORY_CONTEXT>` and the applied memory text.
- LM Studio/Gemma (`gemma-4-e4b-uncensored-hauhaucs-aggressive`) successfully ran through a managed MNO lane and treated insufficient memory as insufficient, then proposed a reviewed memory update instead of claiming silent persistence.

## Not Vendored Wholesale

The clean MNO folder contains additional runtime-side features and docs such as ANN/raw-context sidecars and public integration bundles. Those are runtime-internal MNO improvements, not all CarsinOS gateway integration requirements. The existing vendored MNO runtime already carries CarsinOS-specific runtime and test additions, so wholesale replacement would risk regressing local integration behavior. Further vendor updates should be done as a dedicated MNO vendor-sync workstream with a file-level compatibility map and full MNO pytest coverage.

## Verification

- `cargo test -p carsinos-gateway numquam_ --locked -- --nocapture`
- `npm run test:unit -- src/features/assistant/corePrompt.test.ts`
- `npm run typecheck`
- `cargo fmt --all -- --check`
- `python -m py_compile vendors/mno/engine/runtime/server.py`
- `python -m pytest vendors/mno/tests/integration/test_integration_contract_api.py -q`
- Live proof report: `Z:\carsinos-codex-work\carsinos\reports\execass-mno-agent-context-live-20260602083628\report.json` (`9 pass / 0 fail`)
