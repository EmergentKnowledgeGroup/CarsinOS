#!/usr/bin/env python3
"""Fail-closed guard for disallowed hardcoded runtime values.

Allowlist CSV format (header required):
path_regex,line_regex,owner,expires_on_utc,reason
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Iterable


PATTERNS = {
    "SECRET_TOKEN_LITERAL": re.compile(
        r"(sk-[A-Za-z0-9]{16,}|xoxb-[A-Za-z0-9\-]{16,}|ghp_[A-Za-z0-9]{20,}|AIza[0-9A-Za-z_\-]{20,})"
    ),
    "HARDCODED_WEBHOOK_OR_CALLBACK": re.compile(
        r"(?i)(webhook_url|callback_url|public_base_url)\s*[:=]\s*['\"][^'\"]+['\"]"
    ),
    "HARDCODED_CHANNEL_OR_APP_ID": re.compile(
        r"(?i)(chat_id|guild_id|channel_id|application_id)\s*[:=]\s*['\"][^'\"]+['\"]"
    ),
    "HARDCODED_JWT_ALLOWLIST": re.compile(
        r"(?i)(jwt_issuer_allowlist|jwt_audience_allowlist)\s*[:=]\s*\[[^\]]+\]"
    ),
}


EXCLUDED_PREFIXES = (
    ".git/",
    "target/",
    "runtime/",
    "docs/",
)

EXCLUDED_SUFFIXES = (
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".pdf",
    ".sqlite",
    ".db",
)


@dataclass
class AllowlistRule:
    path_regex: re.Pattern[str]
    line_regex: re.Pattern[str]
    owner: str
    expires_on: date
    reason: str


@dataclass
class Finding:
    pattern_id: str
    path: str
    line_number: int
    line_text: str


def git_tracked_files(repo_root: Path) -> list[str]:
    proc = subprocess.run(
        ["git", "ls-files"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line.strip() for line in proc.stdout.splitlines() if line.strip()]


def should_scan(path: str) -> bool:
    normalized = path.replace("\\", "/")
    if any(normalized.startswith(prefix) for prefix in EXCLUDED_PREFIXES):
        return False
    if any(normalized.endswith(suffix) for suffix in EXCLUDED_SUFFIXES):
        return False
    if "/tests/" in normalized:
        return False
    if normalized.startswith("migrations/"):
        return False
    return True


def load_allowlist(path: Path) -> tuple[list[AllowlistRule], list[str]]:
    rules: list[AllowlistRule] = []
    errors: list[str] = []
    if not path.exists():
        return rules, errors

    with path.open("r", encoding="utf-8", newline="") as fh:
        reader = csv.DictReader(
            row for row in fh if row.strip() and not row.lstrip().startswith("#")
        )
        expected = ["path_regex", "line_regex", "owner", "expires_on_utc", "reason"]
        if reader.fieldnames != expected:
            errors.append(
                f"allowlist header mismatch in {path}: expected {expected}, got {reader.fieldnames}"
            )
            return rules, errors

        for idx, row in enumerate(reader, start=2):
            try:
                expires_on = date.fromisoformat(row["expires_on_utc"].strip())
                rules.append(
                    AllowlistRule(
                        path_regex=re.compile(row["path_regex"].strip()),
                        line_regex=re.compile(row["line_regex"].strip()),
                        owner=row["owner"].strip(),
                        expires_on=expires_on,
                        reason=row["reason"].strip(),
                    )
                )
            except Exception as exc:  # noqa: BLE001
                errors.append(f"invalid allowlist row {idx}: {exc}")
    return rules, errors


def is_allowlisted(finding: Finding, rules: Iterable[AllowlistRule]) -> tuple[bool, str | None]:
    today = datetime.now(timezone.utc).date()
    for rule in rules:
        if rule.path_regex.search(finding.path) and rule.line_regex.search(finding.line_text):
            if rule.expires_on < today:
                return False, f"expired allowlist (owner={rule.owner}, expires={rule.expires_on})"
            return True, None
    return False, None


def collect_findings(repo_root: Path, files: Iterable[str]) -> list[Finding]:
    findings: list[Finding] = []
    for rel in files:
        if not should_scan(rel):
            continue
        full = repo_root / rel
        if not full.exists() or not full.is_file():
            continue
        try:
            text = full.read_text(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            continue
        for line_no, line in enumerate(text.splitlines(), start=1):
            for pattern_id, pattern in PATTERNS.items():
                if pattern.search(line):
                    findings.append(
                        Finding(
                            pattern_id=pattern_id,
                            path=rel,
                            line_number=line_no,
                            line_text=line.strip(),
                        )
                    )
    return findings


def write_report(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", default=".")
    parser.add_argument(
        "--allowlist",
        default="docs/security/HARDCODED_VALUE_ALLOWLIST.csv",
    )
    parser.add_argument(
        "--report",
        default=None,
        help="Output JSON report path (default runtime/security/reports/hardcoded-value-guard-<ts>.json)",
    )
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_path = (
        Path(args.report)
        if args.report
        else repo_root / "runtime/security/reports" / f"hardcoded-value-guard-{ts}.json"
    )

    files = git_tracked_files(repo_root)
    rules, allowlist_errors = load_allowlist((repo_root / args.allowlist).resolve())
    findings = collect_findings(repo_root, files)

    active_violations = []
    informational = []
    for finding in findings:
        allowed, allow_reason = is_allowlisted(finding, rules)
        payload = {
            "pattern_id": finding.pattern_id,
            "path": finding.path,
            "line_number": finding.line_number,
            "line_text": finding.line_text,
        }
        if allowed:
            informational.append(payload)
        else:
            if allow_reason:
                payload["allowlist_error"] = allow_reason
            active_violations.append(payload)

    status = "green"
    if allowlist_errors or active_violations:
        status = "red"

    report_payload = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "workflow": "hardcoded_value_guard",
        "status": status,
        "repo_root": str(repo_root),
        "allowlist": str((repo_root / args.allowlist).resolve()),
        "allowlist_errors": allowlist_errors,
        "total_findings": len(findings),
        "allowlisted_findings": len(informational),
        "violations": active_violations,
    }
    write_report(report_path, report_payload)

    latest_path = report_path.parent / "hardcoded-value-guard-latest.json"
    latest_path.write_text(json.dumps(report_payload, indent=2), encoding="utf-8")

    print(f"[hardcoded-guard] report: {report_path}")
    print(f"[hardcoded-guard] status: {status}")
    if allowlist_errors:
        for err in allowlist_errors:
            print(f"[hardcoded-guard] allowlist error: {err}")
    if active_violations:
        for item in active_violations[:40]:
            print(
                f"[hardcoded-guard] violation {item['pattern_id']} {item['path']}:{item['line_number']} :: {item['line_text']}"
            )

    return 0 if status == "green" else 1


if __name__ == "__main__":
    sys.exit(main())
