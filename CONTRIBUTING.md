# Contributing to CarsinOS

Thanks for helping make local AI operations clearer, safer, and more useful.
Bug fixes, tests, documentation, accessibility improvements, and focused
feature proposals are welcome.

## Before opening an issue

- Search existing issues first.
- Use the bug form for a reproducible defect and the feature form for a
  user-facing outcome.
- Remove tokens, credentials, private messages, local paths, runtime databases,
  and personal data before posting.
- Use [private vulnerability reporting](https://github.com/EmergentKnowledgeGroup/CarsinOS/security/advisories/new)
  for security issues—never a public issue.

## Development setup

```bash
git clone https://github.com/<your-user>/CarsinOS.git
cd CarsinOS
git remote add upstream https://github.com/EmergentKnowledgeGroup/CarsinOS.git
git fetch upstream
git switch -c your-focused-change upstream/main
cargo test --workspace --locked
```

For Mission Control:

```bash
cd apps/mission-control
npm ci
npm run typecheck
npm run lint
npm run test:unit
npm run build
```

## Pull requests

1. Keep one pull request focused on one outcome.
2. Explain the user-visible change, why it is needed, and any security or
   compatibility effect.
3. Add or update proportionate tests for behavior changes.
4. Run the checks that cover your change and include the exact commands/results.
5. Do not include generated build output, local runtime state, secrets, or
   development planning material.

CarsinOS is intentionally local-first. Preserve explicit confirmation,
loopback, credential, tool-scope, provenance, and recovery boundaries unless
your contribution deliberately strengthens them and proves the result.

By contributing, you agree that your contribution will be licensed under the
[MIT License](LICENSE).
