# CarsinOS Public Release Sprint Checklist

## PRS-0 Contract and baseline

- [x] PRS-0.1 Recover Claude audit evidence and verify core source claims.
- [x] PRS-0.2 Create sprint goal, branch, spec, checklist, and blockerboard.
- [x] PRS-0.3 Capture npm advisory baseline and apply non-breaking lockfile fixes.
- [ ] PRS-0.4 Capture initial desktop/mobile browser baseline.

## PRS-1 Stop-ship safety

- [ ] PRS-1.1 Calendar cards select/open details without execution.
- [ ] PRS-1.2 Explicit `Run now` executes exactly once and reports result.
- [ ] PRS-1.3 Add focused unit/E2E regression coverage.
- [ ] PRS-1.4 Verify runbook/task controls never bubble into execution.

## PRS-2 Interaction clarity

- [ ] PRS-2.1 Add accessible toast live-region semantics and tests.
- [ ] PRS-2.2 Verify approval/deny, run/pause, reconnect, and create failures are visible.
- [ ] PRS-2.3 Mark primary/secondary/danger intent in touched critical views.
- [ ] PRS-2.4 Record global button-default normalization as a separately gated follow-up.

## PRS-3 Plain language and list behavior

- [ ] PRS-3.1 Replace user-facing MNO/Principal/Leases/TTL/hierarchy jargon in priority views.
- [ ] PRS-3.2 Move message streams from pagination to a bounded accessible scroll region.
- [ ] PRS-3.3 Preserve pagination for finite lists/tables.
- [ ] PRS-3.4 Verify Help copy matches the actual controls.

## PRS-4 Public auditability

- [x] PRS-4.1 Add MIT license consistent with workspace metadata.
- [x] PRS-4.2 Add vulnerability reporting policy.
- [ ] PRS-4.3 Rewrite README scope/setup/security/release status to current truth.
- [ ] PRS-4.4 Generate fresh security and quality evidence.
- [ ] PRS-4.5 Verify secrets, personal data, and runtime artifacts are excluded from Git.

## PRS-5 Release proof

- [ ] PRS-5.1 Desktop and mobile browser QA with screenshots.
- [ ] PRS-5.2 Frontend full validation green.
- [ ] PRS-5.3 Rust scoped/full validation green.
- [ ] PRS-5.4 PR/security workflows run automatically for `main` pull requests.
- [ ] PRS-5.5 Review, merge, publish release, and change visibility only when blockerboard is green.
