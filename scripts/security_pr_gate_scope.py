#!/usr/bin/env python3
"""Decide whether a pull request needs the full Rust security gate."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any


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


def evaluate_github_files(
    payload: Any, expected_count: int
) -> tuple[bool, str]:
    pages = payload if isinstance(payload, list) else []
    if pages and all(isinstance(page, list) for page in pages):
        files = [item for page in pages for item in page]
    else:
        files = pages

    if not isinstance(files, list) or not all(isinstance(item, dict) for item in files):
        return True, "GitHub returned an invalid PR file list; running the heavy gate fail-safe."
    if len(files) != expected_count:
        return True, (
            f"GitHub reported {expected_count} changed file(s), but the files API returned "
            f"{len(files)}; running the heavy gate fail-safe."
        )

    paths: list[str] = []
    for item in files:
        filename = item.get("filename")
        previous_filename = item.get("previous_filename")
        if isinstance(filename, str) and filename.strip():
            paths.append(filename)
        if isinstance(previous_filename, str) and previous_filename.strip():
            paths.append(previous_filename)
    return evaluate(paths)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--github-files-json", type=Path)
    parser.add_argument("--expected-count", type=int)
    args = parser.parse_args()

    if args.github_files_json is not None:
        if args.expected_count is None:
            parser.error("--expected-count is required with --github-files-json")
        try:
            payload = json.loads(args.github_files_json.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            print(f"Could not read GitHub's PR file list ({error}); running heavy fail-safe.")
            return 0
        required, explanation = evaluate_github_files(payload, args.expected_count)
    else:
        required, explanation = evaluate(sys.stdin.read().splitlines())
    print(explanation)
    return 0 if required else 1


if __name__ == "__main__":
    raise SystemExit(main())
