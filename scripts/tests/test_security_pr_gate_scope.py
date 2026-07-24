import unittest

from scripts.security_pr_gate_scope import (
    evaluate,
    evaluate_github_files,
    security_relevant_path,
)


class SecurityPrGateScopeTests(unittest.TestCase):
    def test_frontend_and_docs_do_not_require_heavy_gate(self) -> None:
        required, _ = evaluate(
            [
                "apps/mission-control/src/features/calendar/CalendarPage.tsx",
                "docs/plans/handoff.md",
            ]
        )
        self.assertFalse(required)

    def test_rust_cargo_tauri_contract_and_gate_changes_require_it(self) -> None:
        for path in (
            "crates/carsinos-gateway/src/main.rs",
            "Cargo.lock",
            "apps/mission-control/src-tauri/src/main.rs",
            "contracts/execass/openapi.json",
            "scripts/security_pr_gate.sh",
            "scripts/security_pr_gate_scope.py",
            ".github/workflows/pr-gate.yml",
        ):
            with self.subTest(path=path):
                self.assertTrue(security_relevant_path(path))

    def test_mixed_change_set_requires_heavy_gate(self) -> None:
        required, explanation = evaluate(
            ["docs/readme.md", "crates/carsinos-storage/src/lib.rs"]
        )
        self.assertTrue(required)
        self.assertIn("carsinos-storage", explanation)

    def test_empty_change_list_fails_safe(self) -> None:
        required, explanation = evaluate([])
        self.assertTrue(required)
        self.assertIn("fail-safe", explanation)

    def test_rename_checks_previous_and_current_paths(self) -> None:
        required, explanation = evaluate_github_files(
            [
                [
                    {
                        "filename": "docs/retired-backend-note.md",
                        "previous_filename": "crates/carsinos-core/src/lib.rs",
                    }
                ]
            ],
            expected_count=1,
        )
        self.assertTrue(required)
        self.assertIn("carsinos-core", explanation)

    def test_complete_github_file_list_can_take_fast_path(self) -> None:
        required, _ = evaluate_github_files(
            [[{"filename": "apps/mission-control/src/App.tsx"}, {"filename": "docs/ui.md"}]],
            expected_count=2,
        )
        self.assertFalse(required)

    def test_truncated_github_file_list_fails_safe(self) -> None:
        required, explanation = evaluate_github_files(
            [[{"filename": "docs/first.md"}]],
            expected_count=3001,
        )
        self.assertTrue(required)
        self.assertIn("3001", explanation)


if __name__ == "__main__":
    unittest.main()
