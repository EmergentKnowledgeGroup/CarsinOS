# Contributing to CarsinOS

Thanks for helping make local AI operations clearer, safer, and more useful.
Focused bug fixes, tests, documentation, accessibility improvements, and
well-bounded feature proposals are welcome.

Before starting, get the product model from [Concepts](docs/CONCEPTS.md) and
the repository map from [Engineering](docs/ENGINEERING.md). The fastest route
to a useful contribution is to preserve the owner-facing truth rather than add
a second source of state or a cosmetic workaround.

## Before opening an issue

- Search existing issues first.
- Use the bug form for a reproducible defect and the feature form for a
  user-facing outcome.
- Remove tokens, credentials, private messages, local paths, runtime databases,
  and personal data before posting.
- Use [private vulnerability reporting](https://github.com/EmergentKnowledgeGroup/CarsinOS/security/advisories/new)
  for security issues—never a public issue.

## Development setup

~~~powershell
git clone https://github.com/<your-user>/CarsinOS.git
cd CarsinOS
git remote add upstream https://github.com/EmergentKnowledgeGroup/CarsinOS.git
git fetch upstream
git switch -c your-focused-change upstream/main
~~~

For Mission Control:

~~~powershell
cd apps/mission-control
npm ci
npm run dev
~~~

Read [Getting started](docs/GETTING_STARTED.md) before assuming a browser or
debug-Tauri session can perform native owner-signed ExecAss mutations. The
broader command map is in [Engineering](docs/ENGINEERING.md).

## Pick a contribution shape

| Change | What good looks like |
| --- | --- |
| Bug fix | A reproducible before/after behavior, targeted coverage, and no unrelated cleanup. |
| UI improvement | Desktop and narrow-width evidence, keyboard/focus consideration, and no accidental shared-shell regression. |
| Backend or protocol change | Clear authority ownership, explicit failure/recovery behavior, and contract-aware tests. |
| Documentation | Source-grounded language, useful navigation, and no private runtime or planning material. |
| Feature proposal | A concrete owner outcome, boundaries, and a reason it belongs in CarsinOS rather than a generic wishlist. |

## Pull requests

1. Keep one pull request focused on one outcome.
2. Explain the user-visible change, why it is needed, and any security or
   compatibility effect.
3. Add or update proportionate tests for behavior changes.
4. Run the checks that cover your change and include the exact commands/results.
5. Include screenshots or browser evidence for visual changes.
6. Do not include generated build output, local runtime state, secrets, or
   internal planning material.

For common project checks:

~~~powershell
# Repository root
cargo fmt --all -- --check
cargo test --workspace --locked
~~~

~~~powershell
# apps/mission-control
npm run typecheck
npm run lint
npm run test:unit
npm run build
~~~

Run the browser suite for behavior that needs browser evidence:

~~~powershell
npm run test:e2e:core
~~~

## Boundaries worth preserving

CarsinOS is intentionally local-first. Preserve explicit confirmation,
loopback, credential, tool-scope, provenance, and recovery boundaries unless
your contribution deliberately changes one and proves the result.

The product is also intentionally owner-directed: direct ordinary owner
instructions proceed through their normal path; dangerous ExecAss actions get
one exact, concrete confirmation rather than a repeated veto. Do not turn
either principle into a vague UI slogan—trace it to the actual behavior you
change.

By contributing, you agree that your contribution will be licensed under the
[MIT License](LICENSE).
