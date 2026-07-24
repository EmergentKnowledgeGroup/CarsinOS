import unittest

from scripts.security_pr_gate_scope import evaluate, security_relevant_path


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


if __name__ == "__main__":
    unittest.main()
