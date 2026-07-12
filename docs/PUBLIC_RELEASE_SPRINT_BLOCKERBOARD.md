# CarsinOS Public Release Sprint Blockerboard

| ID | Severity | State | Blocker | Release gate |
|---|---|---|---|---|
| PUB-001 | S0 | OPEN | GitHub repository remains private | Must be explicit final action after all proof is green |
| PUB-002 | S0 | CLOSED | Root license missing | MIT license added, matching Cargo workspace metadata |
| PUB-003 | S0 | CLOSED | No vulnerability reporting policy | Root `SECURITY.md` added |
| UX-001 | S1 | OPEN | Calendar cards execute on exploratory click | Details-before-run plus regression proof required |
| QA-001 | S1 | OPEN | No fresh desktop/mobile proof | Screenshot and interaction evidence required |
| SEC-001 | S1 | IN_PROGRESS | Dependency advisories | Lockfile fixed to zero; full validation pending |
| CI-001 | S1 | OPEN | Release workflows are manual-only | PR triggers and fresh artifacts required |
| UX-002 | S2 | OPEN | Primary-by-default action hierarchy | Critical-view intent sweep now; global inversion separately gated |
| UX-003 | S2 | OPEN | Jargon and chat pagination | Priority plain-language and scroll-region slice required |

No release or public visibility change is allowed while an S0 or S1 row is open.
