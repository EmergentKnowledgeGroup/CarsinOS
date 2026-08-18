#!/usr/bin/env python3
"""Validate Mission Control phase acceptance matrix coverage.

Checks that every Section 7 bullet tagged for the requested phase exists in the
matrix and has at least one concrete automated assertion reference.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


def resolve_repo_root(script_path: Path) -> Path:
    return script_path.resolve().parent.parent


def ensure_inside_repo(repo_root: Path, candidate: Path, label: str) -> tuple[Path | None, str | None]:
    resolved_repo = repo_root.resolve()
    resolved_candidate = candidate.resolve()
    try:
        resolved_candidate.relative_to(resolved_repo)
    except ValueError:
        return None, f"{label} path escapes repo root: {resolved_candidate}"
    return resolved_candidate, None


def parse_phase_bullets(spec_path: Path, phase: str) -> list[str]:
    lines = spec_path.read_text(encoding="utf-8").splitlines()
    in_section_7 = False
    bullets: list[str] = []

    pattern = re.compile(r"^- \[(P\d+)\] (.+)$")

    for raw_line in lines:
        line = raw_line.strip()
        if line.startswith("## 7)"):
            in_section_7 = True
            continue
        if in_section_7 and line.startswith("## "):
            break
        if not in_section_7:
            continue

        match = pattern.match(line)
        if not match:
            continue
        if match.group(1) != phase:
            continue
        bullets.append(match.group(2).strip())

    return bullets


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def read_assertion_file(repo_root: Path, rel_path: str) -> tuple[Path | None, str | None]:
    if not rel_path.strip():
        return None, "assertion.file must be non-empty"
    full_path = (repo_root / rel_path).resolve()
    try:
        full_path.relative_to(repo_root)
    except ValueError:
        return None, f"assertion file escapes repo root: {rel_path}"
    if not full_path.exists() or not full_path.is_file():
        return None, f"assertion file missing: {rel_path}"
    return full_path, None


def validate_assertion(
    *,
    repo_root: Path,
    bullet_text: str,
    idx: int,
    assertion: dict[str, Any],
) -> list[str]:
    errors: list[str] = []

    file_value = assertion.get("file")
    if not isinstance(file_value, str):
        return [f"{bullet_text}: assertion[{idx}] missing string field 'file'"]

    file_path, file_error = read_assertion_file(repo_root, file_value)
    if file_error:
        return [f"{bullet_text}: assertion[{idx}] {file_error}"]
    assert file_path is not None

    content = file_path.read_text(encoding="utf-8")

    test_name = assertion.get("test_name")
    if test_name is not None:
        if not isinstance(test_name, str) or not test_name.strip():
            errors.append(f"{bullet_text}: assertion[{idx}] invalid test_name")
        elif test_name not in content:
            errors.append(
                f"{bullet_text}: assertion[{idx}] test_name not found in {file_value}: {test_name!r}"
            )

    for field_name in ("present_substrings", "absent_substrings"):
        raw = assertion.get(field_name, [])
        if raw is None:
            raw = []
        if not isinstance(raw, list) or not all(isinstance(item, str) for item in raw):
            errors.append(f"{bullet_text}: assertion[{idx}] {field_name} must be a string array")
            continue
        for token in raw:
            if not token:
                errors.append(f"{bullet_text}: assertion[{idx}] {field_name} includes empty value")
                continue
            if field_name == "present_substrings" and token not in content:
                errors.append(
                    f"{bullet_text}: assertion[{idx}] required substring missing in {file_value}: {token!r}"
                )
            if field_name == "absent_substrings" and token in content:
                errors.append(
                    f"{bullet_text}: assertion[{idx}] forbidden substring present in {file_value}: {token!r}"
                )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Mission Control phase acceptance matrix")
    parser.add_argument("--phase", required=True, help="Phase tag, for example P1")
    parser.add_argument(
        "--spec",
        default="docs/Reliability_OpsUX_upgrade.md",
        help="Path to source spec markdown (default: docs/Reliability_OpsUX_upgrade.md)",
    )
    parser.add_argument(
        "--matrix",
        default=None,
        help="Path to acceptance matrix JSON (default: docs/mission-control_<phase>_acceptance_matrix.json)",
    )
    args = parser.parse_args()

    phase = args.phase.strip().upper()
    if not re.fullmatch(r"P\d+", phase):
        print(f"invalid phase: {args.phase}", file=sys.stderr)
        return 2

    script_path = Path(__file__).resolve()
    repo_root = resolve_repo_root(script_path)

    spec_candidate = repo_root / args.spec
    spec_path, spec_error = ensure_inside_repo(repo_root, spec_candidate, "spec")
    if spec_error:
        print(spec_error, file=sys.stderr)
        return 2
    assert spec_path is not None
    if not spec_path.exists():
        print(f"spec file not found: {spec_path}", file=sys.stderr)
        return 2

    default_matrix = f"docs/mission-control_{phase.lower()}_acceptance_matrix.json"
    matrix_arg = args.matrix or default_matrix
    matrix_candidate = repo_root / matrix_arg
    matrix_path, matrix_error = ensure_inside_repo(repo_root, matrix_candidate, "matrix")
    if matrix_error:
        print(matrix_error, file=sys.stderr)
        return 2
    assert matrix_path is not None
    if not matrix_path.exists():
        print(f"matrix file not found: {matrix_path}", file=sys.stderr)
        return 2

    try:
        spec_bullets = parse_phase_bullets(spec_path, phase)
        matrix_raw = load_json(matrix_path)
    except Exception as error:
        print(f"failed to read inputs: {error}", file=sys.stderr)
        return 2

    if not isinstance(matrix_raw, dict):
        print("matrix root must be a JSON object", file=sys.stderr)
        return 2

    matrix = matrix_raw

    if not spec_bullets:
        print(f"no bullets found in Section 7 for phase {phase}", file=sys.stderr)
        return 2

    matrix_phase = matrix.get("phase")
    if matrix_phase != phase:
        print(
            f"matrix phase mismatch: expected {phase}, got {matrix_phase!r}",
            file=sys.stderr,
        )
        return 2

    bullet_entries = matrix.get("bullets")
    if not isinstance(bullet_entries, list):
        print("matrix.bullets must be an array", file=sys.stderr)
        return 2

    errors: list[str] = []
    matrix_text_to_entry: dict[str, dict[str, Any]] = {}
    for entry in bullet_entries:
        if not isinstance(entry, dict):
            errors.append("matrix.bullets contains non-object entry")
            continue
        text = entry.get("text")
        if not isinstance(text, str) or not text.strip():
            errors.append("matrix bullet missing non-empty text")
            continue
        if text in matrix_text_to_entry:
            errors.append(f"duplicate matrix bullet text: {text}")
            continue
        matrix_text_to_entry[text] = entry

    spec_set = set(spec_bullets)
    matrix_set = set(matrix_text_to_entry.keys())
    missing = sorted(spec_set - matrix_set)
    extra = sorted(matrix_set - spec_set)
    if missing:
        errors.append(f"missing matrix bullets: {missing}")
    if extra:
        errors.append(f"matrix has extra bullets not in spec Section 7 [{phase}]: {extra}")

    for bullet_text in sorted(spec_set):
        entry = matrix_text_to_entry.get(bullet_text)
        if entry is None:
            continue
        assertions = entry.get("assertions")
        if not isinstance(assertions, list) or len(assertions) == 0:
            errors.append(f"{bullet_text}: must include at least one assertion")
            continue
        for idx, assertion in enumerate(assertions):
            if not isinstance(assertion, dict):
                errors.append(f"{bullet_text}: assertion[{idx}] must be an object")
                continue
            errors.extend(
                validate_assertion(
                    repo_root=repo_root,
                    bullet_text=bullet_text,
                    idx=idx,
                    assertion=assertion,
                )
            )

    if errors:
        print("Mission Control phase acceptance matrix check: FAIL")
        for message in errors:
            print(f"- {message}")
        return 1

    print("Mission Control phase acceptance matrix check: PASS")
    print(f"- phase: {phase}")
    print(f"- spec bullets: {len(spec_bullets)}")
    print(f"- matrix file: {matrix_path.relative_to(repo_root)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
