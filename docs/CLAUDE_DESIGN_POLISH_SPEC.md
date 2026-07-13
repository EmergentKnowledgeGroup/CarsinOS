# Claude Design Polish Spec

Status: locked for implementation

## Goal

Make Mission Control understandable to a lightly trained first-time operator while preserving every runtime, security, approval, scheduling, persistence, and navigation contract already protected on `main`.

## Non-negotiable behavior boundaries

- Selecting or inspecting an object never performs its mutation.
- Calendar jobs run only through an explicit action.
- Approval, send, create, update, reconnect, import, publish, and run failures are visible.
- No backend API, storage schema, auth, provider, routing, or execution-policy changes unless a verified UI defect cannot be fixed without one.
- Existing feature flags, deep links, keyboard behavior, reduced motion, compact density, and theme families remain supported.
- Shared/global UI changes require desktop and mobile browser proof.

## Design contract

- One visually dominant action per view or modal; ordinary buttons are neutral by default.
- Plain language first. Technical identifiers are secondary, copyable details—not primary labels.
- Lists select; details explain; explicit actions mutate.
- Long panel content scrolls inside its owning panel. Pagination is reserved for genuinely large finite collections, not chat streams.
- Empty, loading, error, and success states tell the operator what happened and what to do next.
- Mobile controls are at least 44 by 44 CSS pixels, layouts collapse without horizontal page overflow, and essential content comes first.
- Reuse shared primitives instead of page-specific copies for summaries, state panels, empty states, busy labels, master-detail layouts, and feedback.

## Required audit closure

1. Explicit button hierarchy and marking sweep.
2. Visible feedback for mutating actions, including Focus approvals.
3. Calendar details-before-run remains protected.
4. Rooms owns its filters and message reactions target messages or are removed until truthful.
5. File-lock controls are named and placed as operator tooling, not unexplained messaging jargon.
6. Product-wide plain-language and raw-ID humanization pass.
7. Message streams scroll naturally; finite collections use pagination deliberately.
8. Strategy, Runbook, Memory, and Connectors adopt a consistent master-detail mental model where applicable.
9. Duplicate summary/state/empty/busy primitives are consolidated.
10. Cockpit, onboarding, Team, Help, Boards, and shell density/navigation drift are polished without behavior regression.

## Visual proof contract

- Deterministic populated and empty/error states for every affected surface.
- Screenshots at 375x812, 768x1024, 1024x768, and 1440x900.
- Assertions for page overflow, clipped text, touch targets, modal containment, console errors, selection-versus-mutation, and focus restoration.
- Before/after evidence stored under `runtime/reports/claude-design-polish/` and ignored by Git.

## Completion gate

No item is complete from source inspection alone. Completion requires browser evidence, focused regression coverage, the full Mission Control suite, Rust/security gates, independent PR CI, and a clean merge to `main`.
