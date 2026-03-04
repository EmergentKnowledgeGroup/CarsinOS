#!/usr/bin/env python3
"""Mission Control quality gate runner.

Config-driven gate profiles with optional blocker-aware step suppression.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import signal
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


@dataclass
class StepResult:
    step_id: str
    status: str  # PASS | FAIL | BLOCKED
    command: list[str]
    cwd: str
    blocked_by: list[str]
    blocker_reason: str | None
    return_code: int | None
    duration_ms: int


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def normalize_command(raw: Any) -> list[str]:
    if isinstance(raw, list) and raw and all(isinstance(x, str) for x in raw):
        return raw
    if isinstance(raw, str) and raw.strip():
        return shlex.split(raw)
    raise ValueError(f"invalid command: {raw!r}")


def parse_active_blockers(raw_values: list[str]) -> set[str]:
    out: set[str] = set()
    for raw in raw_values:
        for part in raw.split(","):
            cleaned = part.strip()
            if cleaned:
                out.add(cleaned)
    return out


def resolve_profile(config: dict[str, Any], profile_name: str) -> list[dict[str, Any]]:
    profiles = config.get("profiles")
    if not isinstance(profiles, dict):
        raise ValueError("config.profiles must be an object")
    if profile_name not in profiles:
        raise ValueError(f"unknown profile '{profile_name}'")

    visited: set[str] = set()

    def collect(name: str) -> list[dict[str, Any]]:
        if name in visited:
            raise ValueError(f"cyclic profile extends detected at '{name}'")
        visited.add(name)
        profile = profiles.get(name)
        if not isinstance(profile, dict):
            raise ValueError(f"profile '{name}' must be an object")
        steps: list[dict[str, Any]] = []
        extends = profile.get("extends")
        if extends is not None:
            if not isinstance(extends, str) or not extends:
                raise ValueError(f"profile '{name}'.extends must be a non-empty string")
            steps.extend(collect(extends))
        own_steps = profile.get("steps")
        if not isinstance(own_steps, list):
            raise ValueError(f"profile '{name}'.steps must be an array")
        for step in own_steps:
            if not isinstance(step, dict):
                raise ValueError(f"profile '{name}' has non-object step")
            steps.append(step)
        return steps

    return collect(profile_name)


def resolve_repo_root(script_path: Path) -> Path:
    return script_path.resolve().parent.parent


def run_step(step: dict[str, Any], repo_root: Path, log_handle, active_blockers: set[str]) -> StepResult:
    from time import perf_counter

    step_id = str(step.get("id", "")).strip()
    if not step_id:
        raise ValueError("step.id is required")

    command = normalize_command(step.get("command"))

    cwd_rel = str(step.get("cwd", ".")).strip() or "."
    cwd = (repo_root / cwd_rel).resolve()
    try:
        cwd.relative_to(repo_root)
    except ValueError as error:
        raise ValueError(f"step '{step_id}' cwd is invalid: {cwd_rel}") from error
    if not cwd.exists() or not cwd.is_dir():
        raise ValueError(f"step '{step_id}' cwd is invalid: {cwd_rel}")

    timeout_raw = step.get("timeout_sec", 300)
    try:
        timeout_sec = float(timeout_raw)
    except (TypeError, ValueError) as error:
        raise ValueError(f"step '{step_id}' timeout_sec must be a number > 0") from error
    if timeout_sec <= 0:
        raise ValueError(f"step '{step_id}' timeout_sec must be a number > 0")

    blocked_by_raw = step.get("blocked_by", [])
    if blocked_by_raw is None:
        blocked_by_raw = []
    if not isinstance(blocked_by_raw, list) or not all(isinstance(x, str) for x in blocked_by_raw):
        raise ValueError(f"step '{step_id}' blocked_by must be a string array")

    blocked_by = [x.strip() for x in blocked_by_raw if x.strip()]
    matched_blockers = sorted(set(blocked_by).intersection(active_blockers))
    blocker_reason = None
    if matched_blockers:
        blocker_reason = str(step.get("blocker_reason", "")).strip() or None
        note = f"BLOCKED {step_id} blockers={','.join(matched_blockers)}"
        if blocker_reason:
            note += f" reason={blocker_reason}"
        print(note)
        print(note, file=log_handle)
        return StepResult(
            step_id=step_id,
            status="BLOCKED",
            command=command,
            cwd=str(cwd),
            blocked_by=matched_blockers,
            blocker_reason=blocker_reason,
            return_code=None,
            duration_ms=0,
        )

    start = perf_counter()
    header = f"START {step_id}: {' '.join(shlex.quote(arg) for arg in command)} (cwd={cwd})"
    print(header)
    print(header, file=log_handle)

    proc = subprocess.Popen(
        command,
        cwd=str(cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
        start_new_session=True,
    )
    try:
        output_text, _ = proc.communicate(timeout=timeout_sec)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                output_text, _ = proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                output_text, _ = proc.communicate()
        else:
            proc.kill()
            output_text, _ = proc.communicate()
        if output_text:
            for line in output_text.splitlines(keepends=True):
                print(line, end="")
                print(line, end="", file=log_handle)
        duration_ms = int((perf_counter() - start) * 1000)
        tail = (
            f"FAIL  {step_id} timeout after {timeout_sec}s ({duration_ms}ms)"
            if timeout_sec is not None
            else f"FAIL  {step_id} timeout ({duration_ms}ms)"
        )
        print(tail)
        print(tail, file=log_handle)
        return StepResult(
            step_id=step_id,
            status="FAIL",
            command=command,
            cwd=str(cwd),
            blocked_by=[],
            blocker_reason=None,
            return_code=124,
            duration_ms=duration_ms,
        )

    if output_text:
        for line in output_text.splitlines(keepends=True):
            print(line, end="")
            print(line, end="", file=log_handle)

    code = proc.returncode
    if code is None:
        code = 1

    duration_ms = int((perf_counter() - start) * 1000)
    if code == 0:
        tail = f"PASS  {step_id} ({duration_ms}ms)"
        status = "PASS"
    else:
        tail = f"FAIL  {step_id} rc={code} ({duration_ms}ms)"
        status = "FAIL"

    print(tail)
    print(tail, file=log_handle)

    return StepResult(
        step_id=step_id,
        status=status,
        command=command,
        cwd=str(cwd),
        blocked_by=[],
        blocker_reason=None,
        return_code=code,
        duration_ms=duration_ms,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Mission Control quality gate profiles")
    parser.add_argument("--profile", default="pr", help="gate profile name (default: pr)")
    parser.add_argument("--config", default=None, help="path to gate config JSON")
    parser.add_argument(
        "--active-blockers",
        action="append",
        default=[],
        help="active blocker ids (comma-separated or repeated)",
    )
    parser.add_argument(
        "--clear-config-blockers",
        action="store_true",
        help="ignore config.active_blockers and use only --active-blockers",
    )
    parser.add_argument(
        "--fail-on-blocked",
        action="store_true",
        help="treat blocked steps as gate failures",
    )
    args = parser.parse_args()

    script_path = Path(__file__).resolve()
    repo_root = resolve_repo_root(script_path)

    config_path = (
        Path(args.config).resolve()
        if args.config
        else (repo_root / "apps/mission-control/quality-gate.config.json").resolve()
    )
    if not config_path.exists():
        print(f"quality gate config not found: {config_path}", file=sys.stderr)
        return 2

    try:
        config = load_json(config_path)
        steps = resolve_profile(config, args.profile)
    except Exception as error:  # pragma: no cover - startup path
        print(f"quality gate config error: {error}", file=sys.stderr)
        return 2

    configured_blockers: set[str] = set()
    if not args.clear_config_blockers:
        raw = config.get("active_blockers", [])
        if raw is None:
            raw = []
        if not isinstance(raw, list) or not all(isinstance(x, str) for x in raw):
            print("quality gate config error: active_blockers must be a string array", file=sys.stderr)
            return 2
        configured_blockers = {x.strip() for x in raw if x.strip()}

    cli_blockers = parse_active_blockers(args.active_blockers)
    active_blockers = configured_blockers.union(cli_blockers)

    report_dir = repo_root / "runtime" / "quality-gate" / "reports"
    report_dir.mkdir(parents=True, exist_ok=True)

    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    log_path = report_dir / f"quality-gate-{args.profile}-{ts}.log"
    summary_path = report_dir / f"quality-gate-{args.profile}-{ts}.json"
    latest_path = report_dir / f"quality-gate-{args.profile}-latest.json"

    print(f"Quality gate profile: {args.profile}")
    print(f"Config: {config_path}")
    print(f"Active blockers: {', '.join(sorted(active_blockers)) if active_blockers else '(none)'}")
    print(f"Log: {log_path}")

    results: list[StepResult] = []

    with log_path.open("w", encoding="utf-8") as log_handle:
        print(f"[{now_utc_iso()}] profile={args.profile}", file=log_handle)
        print(f"config={config_path}", file=log_handle)
        print(
            f"active_blockers={','.join(sorted(active_blockers)) if active_blockers else '(none)'}",
            file=log_handle,
        )

        for step in steps:
            try:
                result = run_step(step, repo_root, log_handle, active_blockers)
            except Exception as error:
                print(f"FAIL  startup error: {error}", file=sys.stderr)
                print(f"FAIL  startup error: {error}", file=log_handle)
                result = StepResult(
                    step_id=str(step.get("id", "unknown")),
                    status="FAIL",
                    command=[],
                    cwd="",
                    blocked_by=[],
                    blocker_reason=None,
                    return_code=2,
                    duration_ms=0,
                )
                results.append(result)
                break

            results.append(result)
            if result.status == "FAIL":
                break

    pass_count = sum(1 for item in results if item.status == "PASS")
    fail_count = sum(1 for item in results if item.status == "FAIL")
    blocked_count = sum(1 for item in results if item.status == "BLOCKED")

    summary: dict[str, Any] = {
        "schema": "carsinos.mission_control_quality_gate.v1",
        "generated_at": now_utc_iso(),
        "profile": args.profile,
        "config": str(config_path),
        "active_blockers": sorted(active_blockers),
        "log": str(log_path),
        "stats": {
            "pass": pass_count,
            "fail": fail_count,
            "blocked": blocked_count,
            "total": len(results),
        },
        "steps": [
            {
                "id": item.step_id,
                "status": item.status,
                "command": item.command,
                "cwd": item.cwd,
                "blocked_by": item.blocked_by,
                "blocker_reason": item.blocker_reason,
                "return_code": item.return_code,
                "duration_ms": item.duration_ms,
            }
            for item in results
        ],
    }

    with summary_path.open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")
    with latest_path.open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")

    print(f"Summary: {summary_path}")

    if fail_count > 0:
        print("Quality gate: FAIL")
        return 1
    if blocked_count > 0 and args.fail_on_blocked:
        print("Quality gate: FAIL (blocked steps present and --fail-on-blocked enabled)")
        return 1
    if blocked_count > 0:
        print("Quality gate: PASS WITH BLOCKERS")
        return 0

    print("Quality gate: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
