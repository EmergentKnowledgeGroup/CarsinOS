# Claude Design Polish Blockerboard

| ID | Severity | State | Blocker | Required proof |
|---|---:|---|---|---|
| CDP-B01 | S0 | Closed | Baseline and after-state screenshot evidence captured | 52-image matrix at four viewports |
| CDP-B02 | S0 | Closed | Claude audit claims source-verified and scoped | Spec, checklist, and implementation diff |
| CDP-B03 | S1 | Closed | Global button hierarchy verified across every tab | Four-viewport full-tab Playwright sweep |
| CDP-B04 | S1 | Closed | Focus and feature-disable failures are visible | Controller rejection propagation plus inline alerts |
| CDP-B05 | S1 | Closed | Mail and Rooms use mobile list/detail navigation | Desktop and phone screenshots plus full E2E |
| CDP-B06 | S1 | Closed | Cross-viewport proof complete | 41/41 E2E and overflow assertions green |

Stop-ship rule: any S0 or S1 blocker must be closed or explicitly proven inapplicable before merge.
