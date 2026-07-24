#!/usr/bin/env python3
"""Decide whether a pull request needs the full Rust security gate."""

from __future__ import annotations

import sys


EXACT_SECURITY_PATHS = {
    ".github/workflows/pr-gate.yml",
    "Cargo.lock",
    "Cargo.toml",
    "audit.toml",
    "deny.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    "scripts/security_hardcoded_value_guard.py",
    "scripts/security_pr_gate.sh",
    "scripts/security_pr_gate_scope.py",
}

SECURITY_PREFIXES = (
    ".cargo/",
    "apps/mission-control/src-tauri/",
    "contracts/",
    "crates/",
)


def security_relevant_path(path: str) -> bool:
    normalized = path.strip().replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return (
        normalized in EXACT_SECURITY_PATHS
        or normalized.endswith(".rs")
        or normalized.startswith(SECURITY_PREFIXES)
    )


def evaluate(paths: list[str]) -> tuple[bool, str]:
    normalized = [path.strip() for path in paths if path.strip()]
    if not normalized:
        return True, "No changed files were reported; running the heavy gate fail-safe."

    relevant = [path for path in normalized if security_relevant_path(path)]
    if relevant:
        preview = ", ".join(relevant[:5])
        suffix = "" if len(relevant) <= 5 else f" (+{len(relevant) - 5} more)"
        return True, f"Security-sensitive changes detected: {preview}{suffix}"

    return False, (
        f"{len(normalized)} changed file(s) are outside the Rust/security boundary; "
        "the heavy gate is not required."
    )


def main() -> int:
    required, explanation = evaluate(sys.stdin.read().splitlines())
    print(explanation)
    return 0 if required else 1


if __name__ == "__main__":
    raise SystemExit(main())
