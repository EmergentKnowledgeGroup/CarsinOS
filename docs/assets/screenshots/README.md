# Glass Office application captures

These screenshots were captured September 5, 2026 from the actual React
application using isolated browser state and the deterministic E2E gateway.
The activity is test data, not customer data or evidence of live model execution.

| Surface | Pixels |
| --- | --- |
| [office](mission-control-office.png) | 1600 x 1000 |
| [office-light](mission-control-office-light.png) | 1600 x 1000 |
| [office-mobile](mission-control-office-mobile.png) | 390 x 844 |
| [window](mission-control-window.png) | 1600 x 1000 |
| [boards](mission-control-boards.png) | 1600 x 1000 |
| [calendar](mission-control-calendar.png) | 1600 x 1000 |
| [staff-directory](mission-control-staff-directory.png) | 1600 x 1000 |
| [setup](mission-control-setup.png) | 1600 x 1000 |
| [policy](mission-control-policy.png) | 1600 x 1000 |
| [onboarding](mission-control-onboarding.png) | 1600 x 1000 |

The [original prototype](glass-office-design-preview.png), recovered and captured
September 4, remains a historical design reference. The gallery above shows the
integrated application. See [implementation status](../../GLASS_OFFICE_STATUS.md).

## Reproduce

From `apps/mission-control`, run `npx playwright test glass-design-integration`.
The test captures both themes, the elevator floors, mobile Office, and first-run
onboarding into `runtime/qa/glass-design`. The core suite separately checks real
UI handlers against the test gateway. Native signed owner actions and live model
responses require a configured desktop runtime and provider.
