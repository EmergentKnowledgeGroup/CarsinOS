# Workflow Lock (Chunk Execution Loop)

This is the required execution loop for ongoing delivery.

1. Checkpoint protocol.
2. Create 3 chunk PRs.
3. For chunk PR #1:
- check CodeRabbit review,
- evaluate fixes/suggestions,
- implement accepted fixes,
- merge/close PR.
4. Repeat step 3 for chunk PR #2 and chunk PR #3.
5. Checkpoint protocol.
6. Repeat until all chunks are merged/closed and repository state matches local files.

## Persistent Overarching Goals (Do Not Drop)

- `/Users/domusanimae/Documents/openclaw replacement/carsinos/APPDEX_IMPLEMENTATION_TICKET_PACK.md`
- `/Users/domusanimae/Documents/openclaw replacement/carsinos/SECURITY_HARDENING_PROGRAM.md`

All future chunk planning and implementation must remain aligned with these two documents.
