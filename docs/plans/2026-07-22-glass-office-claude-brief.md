# Glass Office — Brief for Claude

Proceed with the Glass Office frontend from `docs/plans/2026-07-22-glass-office-design.md` and the checked ExecAss handoff/contract. P0 through P2 are unblocked now.

Keep these decisions fixed:

- The generated ExecAss OpenAPI contains 16 operations. Treat the generated contract and schemas as authoritative.
- CarsinOS Agent Mail already exists and remains the canonical internal agent-to-agent communication system. Office Chatter is a focused, read-first view over Agent Mail rooms plus a backend safe working-note producer. Do not create a second message truth.
- `Z:\COLACK` is an optional Rust/code/display-pattern donor only. Office Chatter does not need COLACK branding, Postgres, webhooks, or a COLACK connector.
- The elevator is a registry renderer, not four hardcoded route branches. Floors and rooms must have stable IDs and data-driven label, order, icon, shortcut, default room, capability requirements, and visibility. Adding, removing, hiding, renaming, reordering, or regrouping modules must not require application-shell surgery.
- Teams is the Staff Directory. It shows permanent staff and, once the backend exposes authoritative lineage, nests temporary task workers under the responsible staff member. There is no payroll, salary, payment, spending, tenant, or role subsystem.
- Use deterministic briefing prose from `GET /api/v1/execass/summary` for now.
- Do not double-post board moves to ExecAss intake. Keep normal board behavior until the backend supplies a durable observation seam.
- Incident posture is a tested client composition over authoritative health, attention, failed/partial outcome, integrity, runtime-host, breaker/connector, and stop-all facts. Ordinary waiting or an intentional stop is not an incident.
- No frontend lifecycle, approval, danger, proof, receipt, scheduler, or orchestration authority. No dual reads. Native proofs stay in the Tauri bridge and are discarded after use.

Graceful staging:

- P0–P2: implement now against the existing contract.
- P3: Reef may show an honest unavailable/unobserved state until floor presence lands; Office Chatter may show its unwired state until the safe Agent Mail producer lands.
- P4: show current agents as staff and omit temporary-worker nesting until authoritative worker lineage lands. Never infer from names or `reports_to`.

The frontend owns the visual system, motion, information hierarchy, responsive behavior, and delight. The backend remains the only source of operational truth.
