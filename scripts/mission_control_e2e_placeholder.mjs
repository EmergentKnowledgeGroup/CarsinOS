#!/usr/bin/env node

const suite = process.argv[2] ?? "unknown";

console.error(
  `[mission-control] e2e suite '${suite}' is not implemented yet. ` +
    "Resolve blocker(s) and replace this placeholder with real Playwright coverage."
);
process.exit(1);
