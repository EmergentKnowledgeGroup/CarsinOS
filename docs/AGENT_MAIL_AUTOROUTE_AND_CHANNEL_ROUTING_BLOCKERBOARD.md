# Agent Mail Auto-Route Blockerboard

Source spec: `docs/AGENT_MAIL_AUTOROUTE_AND_CHANNEL_ROUTING_SPEC.md`

| ID | Severity | Status | Owner | Blocker / Risk | Resolution Gate |
| --- | --- | --- | --- | --- | --- |
| BLK-AMR-001 | P0 | Closed | Gateway | Durable idempotency was underspecified and could allow duplicate replies after restart. | Spec requires `agent-mail-auto:<message_id>:<recipient_agent_id>` persisted through existing runtime state. |
| BLK-AMR-002 | P0 | Closed | Gateway | Auto-generated replies could recursively auto-execute. | Spec requires `auto_execution=true` skip rule plus bounded fanout/context. |
| BLK-AMR-003 | P0 | Closed | Gateway | Mail send handlers could block on long model/API runs. | Spec requires background delivery and a testable synchronous internal helper only for tests. |
| BLK-CHR-001 | P0 | Closed | Channel runtime | Explicit `@agent` routing could run the wrong provider/model. | Spec requires selected agent model/provider defaults unless request overrides are explicit. |
| BLK-CHR-002 | P1 | Closed | Channel runtime | Sticky routes could silently rewrite permanent human assistant assignment. | Spec requires TTL-bound runtime state and no mutation to `routing.assistant_assignments`. |
| BLK-SKILL-001 | P1 | Closed | Codex workflow | The local Codex skill depends on a running CarsinOS gateway and operator token. | Local skill created under `${CODEX_SKILLS_DIR}\carsinos-agent-mail`; env vars and timeout behavior documented; skill validation passed. |
| BLK-API-001 | P1 | Closed | Provider/runtime | API-backed teammates are only executable if represented as run-capable agents. | Gateway test proves unsupported providers return deterministic `recipient_not_runnable` status without creating a fake run/reply. |
