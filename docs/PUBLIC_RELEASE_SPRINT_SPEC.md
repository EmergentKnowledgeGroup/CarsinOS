# CarsinOS Public Release Sprint Spec

Status: locked for execution on 2026-07-12.

## Goal

Make CarsinOS safe to demonstrate, publicly auditable, and ready for a user-facing release without changing its core runtime contracts.

## Required outcomes

1. Calendar job cards show details before any execution; only an explicit `Run now` action executes.
2. Mutating actions report success and failure through the existing notice/toast path, with accessible live-region semantics.
3. High-traffic user copy prefers plain language; diagnostic identifiers remain available behind context or disclosure.
4. Conversation streams scroll naturally; pagination remains for finite lists and tables.
5. Desktop and mobile browser evidence covers Calendar, Focus, Mail, Rooms, and the application shell.
6. Root legal/security/public-readiness documentation exists and matches behavior.
7. Frontend and Rust security, quality, and release gates produce fresh evidence.
8. GitHub becomes public only after every stop-ship blocker is green.

## Scope fences

- Preserve backend API and storage contracts unless a verified release blocker requires a narrow fix.
- Do not globally invert all button styling without a complete action-intent inventory and visual proof.
- Do not use `npm audit fix --force`, loosen tests, or hide failures.
- Do not mutate or delete the historical UNC checkout.
- Do not add unrelated product features.

## Acceptance gates

- Selecting a Calendar card emits zero run calls; explicit `Run now` emits exactly one.
- Failed approvals, job actions, reconnects, and creates never look successful.
- No relevant console errors, keyboard traps, horizontal overflow, or clipped controls at 375x812 and 1440x900.
- `npm audit` and `npm audit --omit=dev` report zero known advisories.
- Frontend lint, typecheck, full unit tests, core/full E2E, and production build pass.
- Rust formatting, tests selected by changed scope, and security scripts pass.
- `LICENSE`, `SECURITY.md`, README status, release checklist, and evidence paths are current.
