# Contributing to CarsinOS

Thanks for helping make local AI operations safer, clearer, and easier to
audit. Focused bug fixes, tests, documentation improvements, accessibility
work, and well-scoped features are welcome.

## Before you start

- Search existing issues and pull requests before opening a duplicate.
- Open an issue first for large features, architecture changes, or behavior
  that changes a security boundary.
- Never include credentials, private runtime state, personal data, production
  logs, or vulnerability details in a public issue or pull request.
- Report security problems through [GitHub private vulnerability
  reporting](https://github.com/EmergentKnowledgeGroup/CarsinOS/security/advisories/new).

## Development setup

You need a Rust toolchain. Mission Control development also requires Node.js.

```bash
git clone https://github.com/EmergentKnowledgeGroup/CarsinOS.git
cd CarsinOS
cargo test --workspace --locked
```

Mission Control:

```bash
cd apps/mission-control
npm ci
npm run typecheck
npm run lint
npm run test:unit
npm run build
```

## Pull requests

1. Branch from current `main`.
2. Keep one pull request focused on one concern.
3. Add or update tests when behavior changes.
4. Run the checks that cover your change.
5. Explain what changed, why, any security or compatibility impact, and the
   exact validation commands you ran.
6. Target `main` and respond to review feedback on the same branch.

Maintainers use additional local checkpoint files while executing work. Those
files are intentionally ignored and are **not required from external
contributors**.

## Quality expectations

- Prefer clear, deterministic behavior over clever shortcuts.
- Keep modules focused and error handling explicit.
- Avoid unrelated refactors in a feature or bug-fix pull request.
- Preserve loopback, authentication, approval, secret-storage, and tool-scope
  boundaries unless the change explicitly strengthens and tests them.
- Include failure paths and edge cases, not only a happy-path test.

For the maintainer workflow and full local gate set, see
[`docs/GIT_PR_WORKFLOW.md`](docs/GIT_PR_WORKFLOW.md).

## Reporting bugs

Use the bug-report form and include a minimal reproduction, version or commit,
expected behavior, actual behavior, and sanitized logs when useful. Replace
tokens, file paths, usernames, message contents, and third-party identifiers
before posting.

By contributing, you agree that your contribution will be licensed under the
project's [MIT License](LICENSE).
