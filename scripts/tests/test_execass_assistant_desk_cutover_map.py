#!/usr/bin/env python3
"""Static EA-308 guard for the Assistant Desk authority cutover map.

Runs with the Python standard library from any working directory:
    python scripts/tests/test_execass_assistant_desk_cutover_map.py
"""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
MAP_PATH = REPOSITORY_ROOT / "docs" / "EXECASS_EA308_ASSISTANT_DESK_CUTOVER_MAP.json"
LEGACY_TOKEN = re.compile(r"assistant-desk|AssistantDesk|assistant_desk|assistantDesk")
EXACT_LEGACY_SUMMARY_ROUTE = '"/api/v1/assistant-desk"'
NEW_SUMMARY_BUILDER_DECLARATION = re.compile(
    r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+"
    r"(?:build|derive|project)_[a-z0-9_]*(?:execass|executive)[a-z0-9_]*(?:summary|projection)[a-z0-9_]*"
    r"|^\s*(?:pub\s+)?struct\s+(?:ExecAss|Executive)[A-Za-z0-9_]*(?:Summary|Projection)[A-Za-z0-9_]*",
    re.MULTILINE,
)
SOURCE_SUFFIXES = {".css", ".json", ".md", ".mjs", ".py", ".rs", ".ts", ".tsx"}
EXCLUDED_DIRECTORY_NAMES = {
    ".git",
    ".cargo-target",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "runtime",
    "target",
}


def read_text(relative_path: str) -> str:
    return (REPOSITORY_ROOT / relative_path).read_text(encoding="utf-8")


def is_repository_source_file(path: Path) -> bool:
    return (
        path.is_file()
        and path.suffix.lower() in SOURCE_SUFFIXES
        and not any(
            part in EXCLUDED_DIRECTORY_NAMES or part.startswith(".tmp") for part in path.parts
        )
    )


def token_paths() -> set[str]:
    roots = ("apps", "crates", "contracts", "docs", "scripts")
    paths: set[str] = set()
    for root in roots:
        root_path = REPOSITORY_ROOT / root
        if not root_path.exists():
            continue
        for path in root_path.rglob("*"):
            if not is_repository_source_file(path):
                continue
            relative_path = path.relative_to(REPOSITORY_ROOT).as_posix()
            if LEGACY_TOKEN.search(path.read_text(encoding="utf-8", errors="replace")):
                paths.add(relative_path)
    return paths


class AssistantDeskCutoverMapTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.map = json.loads(MAP_PATH.read_text(encoding="utf-8"))

    def test_map_is_pre_ea311_versioned_and_has_open_transcript_decision(self) -> None:
        self.assertEqual(
            self.map["schema"], "carsinos.execass.assistant-desk-cutover.v1"
        )
        self.assertIn(self.map["current_stage"], {"pre_ea311", "post_ea311"})
        self.assertEqual(self.map["planned_replacement"]["endpoint"], "/api/v1/execass/summary")
        self.assertEqual(
            self.map["planned_replacement"]["model"], "ExecAssExecutiveProjection"
        )
        self.assertEqual(self.map["planned_replacement"]["authority"], "storage_projection_only")
        self.assertEqual(self.map["transcript_replacement"]["status"], "OPEN")

    def test_every_current_legacy_token_path_is_accounted_or_typed_excluded(self) -> None:
        accounted = {self.map["legacy_backend"]["path"]}
        accounted.update(item["path"] for item in self.map["direct_frontend_consumers"])
        for item in self.map["distinct_retain"]:
            accounted.update(item["paths"])
        accounted.update(item["path"] for item in self.map["typed_exclusions"])

        discovered = token_paths()
        self.assertSetEqual(
            discovered,
            accounted & discovered,
            "A current Assistant Desk token path is absent from the EA-308 map or typed exclusions.",
        )

    def test_legacy_routes_are_exactly_the_two_fenced_templates(self) -> None:
        gateway_source = read_text(self.map["legacy_backend"]["path"])
        routed_templates = re.findall(r'\.route\(\s*"([^\"]+)"', gateway_source)
        legacy_templates = [
            template for template in routed_templates if template.startswith("/api/v1/assistant-desk")
        ]
        expected = [item["template"] for item in self.map["legacy_routes"]]
        self.assertEqual(legacy_templates, expected)
        self.assertEqual(len(expected), 2)
        self.assertEqual(
            [item["disposition"] for item in self.map["legacy_routes"]],
            ["RETAIN_UNTIL_EA311", "RETAIN_UNTIL_EA311"],
        )
        self.assertIsNotNone(
            re.search(
                r'\.route\(\s*"/api/v1/assistant-desk"\s*,\s*get\(get_assistant_desk\)\s*,?\s*\)',
                gateway_source,
            )
        )
        self.assertIsNotNone(
            re.search(
                r'\.route\(\s*"/api/v1/assistant-desk/\{work_item_id\}/transcript"\s*,\s*get\(get_assistant_desk_transcript\)\s*,?\s*\)',
                gateway_source,
            )
        )

    def test_legacy_backend_symbols_and_tests_are_present(self) -> None:
        gateway_source = read_text(self.map["legacy_backend"]["path"])
        backend = self.map["legacy_backend"]
        for symbol in (
            backend["constants"]
            + backend["dtos"]
            + backend["summary_symbols"]
            + backend["transcript_symbols"]
            + backend["tests"]
        ):
            with self.subTest(symbol=symbol):
                self.assertIn(symbol, gateway_source)

    def test_every_direct_frontend_consumer_has_its_expected_symbols(self) -> None:
        direct_roles = {
            "frontend_api",
            "frontend_controller",
            "frontend_component",
            "frontend_callsite",
            "frontend_e2e_mock",
        }
        for consumer in self.map["direct_frontend_consumers"]:
            with self.subTest(consumer=consumer["id"]):
                source = read_text(consumer["path"])
                self.assertTrue(LEGACY_TOKEN.search(source))
                for symbol in consumer["symbols"]:
                    self.assertIn(symbol, source)
                if consumer["role"] in direct_roles:
                    self.assertIn(
                        consumer.get("disposition", consumer.get("summary_disposition")),
                        {"CUTOVER_AT_EA311", "RETAIN_UNTIL_EA311"},
                    )

    def test_exact_route_callers_and_mock_are_explicitly_accounted(self) -> None:
        consumers_by_path = {
            item["path"]: item for item in self.map["direct_frontend_consumers"]
        }
        expected_paths = {
            "apps/mission-control/src/lib/api.ts",
            "apps/mission-control/e2e/mockGateway.mjs",
        }
        exact_route_paths = {
            path
            for path in (REPOSITORY_ROOT / "apps" / "mission-control").rglob("*")
            if is_repository_source_file(path)
            and EXACT_LEGACY_SUMMARY_ROUTE in path.read_text(encoding="utf-8", errors="replace")
        }
        exact_route_paths = {
            path.relative_to(REPOSITORY_ROOT).as_posix() for path in exact_route_paths
        }
        self.assertSetEqual(exact_route_paths, expected_paths)
        self.assertTrue(expected_paths.issubset(consumers_by_path))
        for path in expected_paths:
            self.assertIn("/api/v1/assistant-desk", consumers_by_path[path]["route_templates"])

    def test_no_new_execass_summary_builder_exists_outside_storage_projection(self) -> None:
        projection_root = self.map["planned_replacement"]["projection_root"]
        offenders: list[str] = []
        for path in (REPOSITORY_ROOT / "crates").rglob("*.rs"):
            relative_path = path.relative_to(REPOSITORY_ROOT).as_posix()
            if relative_path.startswith(projection_root):
                continue
            if NEW_SUMMARY_BUILDER_DECLARATION.search(
                path.read_text(encoding="utf-8", errors="replace")
            ):
                offenders.append(relative_path)
        self.assertEqual(
            offenders,
            [],
            "A new ExecAss summary builder/DTO appeared outside the authoritative storage projection.",
        )

    def test_legacy_summary_builder_cannot_feed_the_planned_execass_route(self) -> None:
        gateway_source = read_text(self.map["legacy_backend"]["path"])
        planned_endpoint = self.map["planned_replacement"]["endpoint"]
        if self.map["current_stage"] == "pre_ea311":
            self.assertNotIn(
                planned_endpoint,
                gateway_source,
                "EA-311 route must not be mounted while the cutover map is pre-EA311.",
            )
            return

        route_start = gateway_source.index(planned_endpoint)
        route_window = gateway_source[route_start : route_start + 600]
        self.assertNotIn("build_assistant_desk_response", route_window)
        self.assertNotIn("AssistantDeskResponse", route_window)

    def test_stage_gate_preserves_current_summary_and_transcript_rules(self) -> None:
        api_source = read_text("apps/mission-control/src/lib/api.ts")
        mock_source = read_text("apps/mission-control/e2e/mockGateway.mjs")
        if self.map["current_stage"] == "pre_ea311":
            self.assertIn(EXACT_LEGACY_SUMMARY_ROUTE, api_source)
            self.assertIn(EXACT_LEGACY_SUMMARY_ROUTE, mock_source)
            return

        self.assertNotIn(EXACT_LEGACY_SUMMARY_ROUTE, api_source)
        self.assertNotIn(EXACT_LEGACY_SUMMARY_ROUTE, mock_source)
        self.assertTrue(self.map["post_ea311_gate"]["summary_route_must_be_removed"])
        self.assertTrue(self.map["post_ea311_gate"]["transcript_retention_allowed"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
