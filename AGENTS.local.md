# AGENTS.local.md

## Local Execution Policy

This file extends `AGENTS.md` for day-to-day implementation in this workspace.

1. Every implementation turn should:
- Read `CHECKPOINT.md`.
- Resume from the referenced checklist item in `CHECKLIST.md`.
- Update `CHECKPOINT.md` after each file-change batch.

2. When blocked:
- Capture blocker in `CHECKPOINT.md` under present action.
- Capture exact missing input needed.
- Stop only if blocker prevents meaningful forward progress.

3. Test gates:
- Run regression tests at every major section boundary.
- Record test command and result in `CHECKPOINT.md`.

4. Scope guard:
- Build from `PLAN.md` + `CHECKLIST.md`.
- Avoid ad hoc feature drift unless required for reliability/security/testability.

