# PinchTab CarsinOS Browser Tool Goal

## Goal Prompt

Fork `pinchtab/pinchtab`, fix the Windows-specific test failures found during the CarsinOS PinchTab research pass, push those fixes to a public fork, and open an upstream pull request. Then add a first-class Rust browser tool layer to CarsinOS that lets ExecAss and platform agents use PinchTab through CarsinOS-native assistant tools, capability declarations, approval gates, audit records, and tests.

## Workstream

Track: `PINCHTAB_FORK_AND_CARSINOS_BROWSER_TOOL WORK`

## Scope

- Keep PinchTab upstream work in the PinchTab fork, on a dedicated branch.
- Keep CarsinOS integration work in the local `carsinos` checkout.
- Do not vendor PinchTab Go internals into CarsinOS.
- Do not replace Mission Control Playwright E2E yet.
- Treat PinchTab as a managed local sidecar reached through a small Rust client.
- Default dangerous browser actions to disabled or approval-gated.

## PinchTab Fork Tasks

- Reproduce the Windows-specific failures:
  - MCP `safeRecordPath` bad-extension test using `/tmp/output.txt`.
  - daemon overview test expecting service text on Windows.
- Patch tests or production wording so behavior is correct on Windows, Linux, and macOS.
- Run focused Go tests for the touched packages.
- Run broader relevant Go tests for MCP, daemon, config, auth, routes, selector, security, and HTTP helpers.
- Build the PinchTab CLI on Windows.
- Commit, push to the public fork, and open a draft upstream PR with validation notes.

## CarsinOS Integration Tasks

- Add a Rust PinchTab client surface for health, session creation, navigation, text, snapshot, capture, and simple actions.
- Add runtime configuration for PinchTab endpoint/token/profile defaults and risky-action gates.
- Expose initial assistant browser tools:
  - `assistant.browser.health`
  - `assistant.browser.navigate`
  - `assistant.browser.text`
  - `assistant.browser.snapshot`
  - `assistant.browser.capture`
- Map CarsinOS agent/run identity to PinchTab session use.
- Route all browser tool calls through existing assistant tool capability and audit infrastructure.
- Keep risky operations disabled or approval-gated:
  - eval
  - cookies
  - downloads
  - uploads
  - clipboard
  - network interception
  - attach mode
  - screencast or recording
  - state import/export

## CarsinOS Tests

- Unit-test PinchTab config defaults and environment overrides.
- Unit-test URL/domain policy decisions.
- Unit-test sensitive-action risk classification.
- Unit-test request/response parsing and token redaction.
- Unit-test assistant capability publication and MCP parity.
- Unit-test assistant tool execution for health/navigate/text/snapshot/capture with a mocked PinchTab endpoint.
- Verify formatting and focused Rust tests.
- Run broader gateway/tool tests when shared assistant-tool contracts change.

## Edge Cases

- PinchTab unavailable.
- PinchTab returns non-JSON or malformed JSON.
- PinchTab returns 401/403.
- Session creation fails.
- Agent attempts browser access without a CarsinOS run/session context.
- Navigation is outside the allowed domain policy.
- Capture/snapshot response is too large.
- Response contains bearer tokens, cookies, or sensitive headers.
- Risky action is requested without approval.
- Windows path and UNC checkout behavior.

## Completion Criteria

- PinchTab fork branch is pushed and an upstream draft PR exists, unless GitHub permissions block PR creation.
- CarsinOS compiles for the touched Rust crates.
- Focused CarsinOS tests prove the new browser client and assistant tools.
- Runtime checkpoints and `CHECKPOINT.md` reflect phase start, post-green tests, PR open, and final state.
- Final response includes exact branches, commits, PR URL if created, validation commands, and any blockers.
