# Project Guidance

## Non-Negotiables
- Reward hacking is not tolerated.
- Fix shared-rule/root-cause failure classes, not benchmark-shaped symptoms.
- Prefer the smallest coherent implementation that satisfies the spec.
- Keep code lean, modular, and reversible.
- Maintain clear no-touch boundaries from the active spec.
- Every feature slice should be backed by TDD-style coverage:
  - write or tighten targeted tests around the intended behavior
  - implement the smallest fix
  - verify regressions on nearby surfaces

## Current Priority Lane
- Active implementation target:
  - `docs/MNO_OPTIONAL_CLAUDE_DRAFT_CURATION_EXECUTION_CHECKLIST_2026-03-16.md`
- This is a weld-in between Build and Review, not a pipeline rewrite.
- Preserve:
  - ingest/orchestrator behavior
  - retrieval/ranking stack
  - publish/verify/activate gates
  - MCP install/activation flows

## Review Standard
- Human review remains authoritative.
- Draft proposals must stay separate from `review_decisions` until explicit promotion.
- Any shortcut that silently mutates review truth, stage truth, or publish truth is a failure.
