# AGENTS.md

## The Workflow

This repository follows a strict execution workflow.

1. Continue building without waiting unless:
- There is a hard blocker.
- Input is 100% required and not already defined in `PLAN.md`.
- The full plan is complete.

2. Maintain a live checkpoint file:
- File: `CHECKPOINT.md`.
- Update it on every codebase file change batch.
- If a single file is edited multiple times, add one checkpoint update for that edit.
- Each checkpoint entry must include:
  - past action (what was done),
  - present action (what is being done now),
  - future action (what is next),
  - current phase/block/section from `CHECKLIST.md`,
  - changed files list.

3. Build from checklist:
- File: `CHECKLIST.md`.
- It must be derived from `PLAN.md`.
- It is the source-of-truth execution order.
- `CHECKPOINT.md` must always reference checklist IDs.

4. Testing discipline:
- At the end of every major checklist section, run full regression tests.
- Tests should be strict and hard to pass.
- Include edge cases and failure paths.
- Do not skip tests without explicit blocker documentation in `CHECKPOINT.md`.

5. Code quality standard:
- Write code as if every line will be audited by a hostile reviewer.
- Optimize for correctness, safety, clarity, and maintainability.
- Justify design choices through clean structure and strong tests.
- Re-evaluate edits before finalizing to catch sub-optimal logic.

6. Preferred implementation style:
- Keep modules focused and composable.
- Avoid hidden coupling and side effects.
- Preserve deterministic behavior and explicit error handling.

