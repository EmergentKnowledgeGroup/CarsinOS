"""Cross-platform contract tests for the EA-104 state tool."""
from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import carsinos_state as state  # noqa: E402


class CarsinosStateToolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="carsinos-state-test-", dir=Path.cwd())
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        (self.source / "attachments").mkdir()
        (self.source / "attachments" / "proof.txt").write_text("proof", encoding="utf-8")
        (self.source / "secrets").mkdir()
        (self.source / "secrets" / "token").write_text("never portable", encoding="utf-8")
        database = sqlite3.connect(self.source / "carsinos.db")
        database.execute("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
        database.execute("CREATE TABLE child (parent_id INTEGER REFERENCES parent(id))")
        database.execute("PRAGMA user_version = 7")
        database.commit()
        database.close()
        (self.source / "receipt-anchor.json").write_text(json.dumps({"state_root_generation": "g-1", "key_id": "key-1"}), encoding="utf-8")
        (self.source / "retry-tombstones.json").write_text('{"retry-1":"blocked"}', encoding="utf-8")
        self.archive = self.root / "state.zip"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_tool(
        self, *arguments: str, environment: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPTS / "carsinos_state.py"), *arguments],
            text=True,
            capture_output=True,
            check=False,
            env=environment,
        )

    def backup(self, source: Path | None = None, archive: Path | None = None) -> None:
        result = self.run_tool("backup", "--state-dir", str(source or self.source), "--archive-path", str(archive or self.archive), "--binary-compatibility-version", "test-v1")
        self.assertEqual(result.returncode, 0, result.stderr)

    def failure_reason(self, result: subprocess.CompletedProcess[str]) -> str:
        self.assertNotEqual(result.returncode, 0)
        payload = json.loads(result.stderr.splitlines()[0])
        self.assertFalse(payload["ok"])
        return payload["reason"]

    def rewrite_manifest(self, destination: Path, mutate) -> None:
        with zipfile.ZipFile(self.archive) as source, zipfile.ZipFile(destination, "w") as rewritten:
            for entry in source.infolist():
                data = source.read(entry.filename)
                if entry.filename == state.MANIFEST_NAME:
                    manifest = json.loads(data)
                    mutate(manifest)
                    data = json.dumps(manifest, sort_keys=True).encode("utf-8")
                rewritten.writestr(entry, data)

    def add_receipt_anchor_row(self, *, state_generation: int | str = 1) -> None:
        database = sqlite3.connect(self.source / "carsinos.db")
        database.execute(
            """
            CREATE TABLE execass_receipt_anchor_state (
              root_identity TEXT NOT NULL,
              state_root_generation,
              anchor_generation INTEGER NOT NULL,
              status TEXT NOT NULL,
              receipt_count INTEGER NOT NULL,
              receipt_head_digest TEXT,
              key_id TEXT NOT NULL,
              key_generation INTEGER NOT NULL,
              transaction_id TEXT NOT NULL,
              external_receipt_digest TEXT NOT NULL
            )
            """
        )
        database.execute(
            """
            INSERT INTO execass_receipt_anchor_state VALUES
              ('sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
               ?,1,'finalized',1,?,'key-one',1,'tx-one',?)
            """,
            (state_generation, "b" * 64, "c" * 64),
        )
        database.commit()
        database.close()

    def test_roundtrip_excludes_secrets_and_manifest_is_complete_and_sorted(self) -> None:
        self.backup()
        verified = self.run_tool("verify", "--archive-path", str(self.archive))
        self.assertEqual(verified.returncode, 0, verified.stderr)
        with zipfile.ZipFile(self.archive) as archive:
            manifest = json.loads(archive.read(state.MANIFEST_NAME))
        files = manifest["files"]
        self.assertEqual([entry["path"] for entry in files], sorted(entry["path"] for entry in files))
        self.assertNotIn("secrets/token", [entry["path"] for entry in files])
        self.assertEqual(manifest["secret_references"][0]["disposition"], "excluded_mandatory_reauthentication")
        self.assertTrue(manifest["retry_tombstones"]["present"])
        self.assertEqual(manifest["sqlite_databases"][0]["user_version"], 7)
        restored = self.root / "restored"
        result = self.run_tool("restore", "--state-dir", str(restored), "--archive-path", str(self.archive), "--expected-binary-compatibility-version", "test-v1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((restored / "attachments" / "proof.txt").read_text(encoding="utf-8"), "proof")
        self.assertFalse((restored / "secrets").exists())

    def test_non_sqlite_database_named_file_is_portable_but_not_receipt_authority(self) -> None:
        fake_database = self.source / "legacy-fixture.db"
        fake_database.write_bytes(b"sqlite-fixture")

        self.backup()

        with zipfile.ZipFile(self.archive) as archive:
            manifest = json.loads(archive.read(state.MANIFEST_NAME))
        self.assertIn("legacy-fixture.db", [item["path"] for item in manifest["files"]])
        self.assertNotIn(
            "legacy-fixture.db",
            [item["path"] for item in manifest["sqlite_databases"]],
        )
        self.assertNotIn(
            "legacy-fixture.db",
            [item["path"] for item in manifest["receipt_metadata"]],
        )

    def test_db_anchor_metadata_is_numeric_and_verifier_contract_binds_both_actions(self) -> None:
        self.add_receipt_anchor_row()
        self.backup()
        with zipfile.ZipFile(self.archive) as archive:
            manifest = json.loads(archive.read(state.MANIFEST_NAME))
        self.assertEqual(manifest["source_state_root_generation"], 1)
        self.assertEqual(manifest["receipt_metadata"][0]["path"], "carsinos.db")
        self.assertEqual(manifest["receipt_metadata"][0]["key_generation"], 1)
        self.assertNotEqual(manifest["receipt_metadata"][0]["path"], "receipt-anchor.json")

        calls: list[dict[str, str]] = []

        def verifier_runner(command: list[str], _cwd: Path) -> subprocess.CompletedProcess[str]:
            action = command[1]
            root = Path(command[3]).resolve()
            calls.append({"action": action, "root": str(root)})
            response = {"ok": True, "action": action}
            if action == "verify-active":
                response.update({
                    "status": "trusted",
                    "root_identity": manifest["receipt_metadata"][0]["root_identity"],
                    "anchor_generation": manifest["receipt_metadata"][0]["anchor_generation"],
                })
            return subprocess.CompletedProcess(command, 0, json.dumps(response) + "\n", "")

        fixed_bundled_path = self.root / "carsinos-receipt-integrity.exe"
        with mock.patch.object(state.subprocess, "run", side_effect=lambda command, **kwargs: verifier_runner(command, kwargs["cwd"])):
            state.run_receipt_verifier(
                self.source, manifest, "inspect-db", fixed_bundled_path
            )
            state.run_receipt_verifier(
                self.source, manifest, "verify-active", fixed_bundled_path
            )
        self.assertEqual([call["action"] for call in calls], ["inspect-db", "verify-active"])
        self.assertEqual([Path(call["root"]) for call in calls], [self.source, self.source])

    def test_execass_anchor_restore_without_verifier_fails_before_activation(self) -> None:
        self.add_receipt_anchor_row()
        self.backup()
        target = self.root / "unverified-target"
        target.mkdir()
        (target / "old.txt").write_text("old", encoding="utf-8")
        result = self.run_tool(
            "restore",
            "--state-dir",
            str(target),
            "--archive-path",
            str(self.archive),
            "--expected-binary-compatibility-version",
            "test-v1",
            "--force",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.failure_reason(result), "receipt_integrity_rejected")
        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "old")
        self.assertEqual(list(self.root.glob("unverified-target.rollback.*")), [])

    def test_cross_root_receipt_archive_restore_runs_and_rolls_back_on_identity_rejection(self) -> None:
        self.add_receipt_anchor_row()
        self.backup()
        target = self.root / "cross-root-target"
        target.mkdir()
        (target / "old.txt").write_text("old", encoding="utf-8")
        fake_verifier = self.root / "carsinos-receipt-integrity.exe"

        def verifier_runner(command: list[str], **_kwargs) -> subprocess.CompletedProcess[str]:
            action = command[1]
            if action == "inspect-db":
                response = {"ok": True, "action": action, "schema": "execass.v1"}
                return subprocess.CompletedProcess(command, 0, json.dumps(response) + "\n", "")
            response = {"ok": False, "error": "receipt_integrity_rejected"}
            return subprocess.CompletedProcess(command, 2, json.dumps(response) + "\n", "")

        with mock.patch.object(state.subprocess, "run", side_effect=verifier_runner):
            stage, _manifest = state.extract_to_stage(
                self.archive,
                target,
                "test-v1",
                fake_verifier,
            )
            with self.assertRaisesRegex(
                state.StateToolError,
                "Receipt-integrity verifier rejected verify-active",
            ):
                state.activate_stage(
                    stage,
                    target,
                    True,
                    state.post_activation_check("test-v1", fake_verifier),
                )

        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "old")
        self.assertEqual(list(self.root.glob("cross-root-target.rollback.*")), [])
        self.assertTrue(self.archive.is_file())

    def test_receipt_verifier_override_option_does_not_exist_in_shipped_cli(self) -> None:
        verifier = self.root / "attacker.py"
        verifier.write_text("print('{}')\n", encoding="utf-8")
        result = self.run_tool(
            "backup",
            "--state-dir",
            str(self.source),
            "--archive-path",
            str(self.archive),
            "--binary-compatibility-version",
            "test-v1",
            "--receipt-integrity-verifier",
            str(verifier),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.failure_reason(result), "invalid_arguments")
        self.assertNotIn(str(verifier), result.stderr)

    def test_receipt_rows_without_anchor_are_rejected_before_manifest_creation(self) -> None:
        self.add_receipt_anchor_row()
        database = sqlite3.connect(self.source / "carsinos.db")
        database.execute("DELETE FROM execass_receipt_anchor_state")
        database.execute("CREATE TABLE execass_receipts (receipt_id TEXT)")
        database.execute("INSERT INTO execass_receipts VALUES ('orphan-receipt')")
        database.commit()
        database.close()
        result = self.run_tool(
            "backup",
            "--state-dir",
            str(self.source),
            "--archive-path",
            str(self.archive),
            "--binary-compatibility-version",
            "test-v1",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.failure_reason(result), "receipt_integrity_rejected")

    def test_non_numeric_database_state_generation_is_rejected(self) -> None:
        self.add_receipt_anchor_row(state_generation="generation-one")
        result = self.run_tool(
            "backup",
            "--state-dir",
            str(self.source),
            "--archive-path",
            str(self.archive),
            "--binary-compatibility-version",
            "test-v1",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.failure_reason(result), "receipt_integrity_rejected")

    def test_tamper_traversal_and_duplicate_entries_are_rejected(self) -> None:
        self.backup()
        tampered = self.root / "tampered.zip"
        with zipfile.ZipFile(self.archive) as source, zipfile.ZipFile(tampered, "w") as destination:
            for entry in source.infolist():
                data = source.read(entry.filename)
                if entry.filename == "attachments/proof.txt":
                    data = b"altered"
                destination.writestr(entry, data)
        self.assertNotEqual(self.run_tool("verify", "--archive-path", str(tampered)).returncode, 0)
        traversal = self.root / "traversal.zip"
        with zipfile.ZipFile(traversal, "w") as archive:
            archive.writestr("../escape", "no")
        self.assertEqual(
            self.failure_reason(self.run_tool("verify", "--archive-path", str(traversal))),
            "archive_validation_rejected",
        )
        duplicate = self.root / "duplicate.zip"
        with zipfile.ZipFile(duplicate, "w") as archive:
            archive.writestr("backup-manifest.json", "{}")
            archive.writestr("backup-manifest.json", "{}")
        self.assertEqual(
            self.failure_reason(self.run_tool("verify", "--archive-path", str(duplicate))),
            "archive_validation_rejected",
        )

    def test_live_pid_blocks_backup_and_force_restore_keeps_rollback(self) -> None:
        (self.source / "gateway.pid").write_text(str(os.getpid()), encoding="utf-8")
        blocked = self.run_tool("backup", "--state-dir", str(self.source), "--archive-path", str(self.archive))
        self.assertEqual(self.failure_reason(blocked), "state_not_quiescent")
        (self.source / "gateway.pid").unlink()
        self.backup()
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_text("old", encoding="utf-8")
        restored = self.run_tool("restore", "--state-dir", str(target), "--archive-path", str(self.archive), "--expected-binary-compatibility-version", "test-v1", "--force")
        self.assertEqual(restored.returncode, 0, restored.stderr)
        rollback = next(self.root.glob("target.rollback.*"))
        self.assertEqual((rollback / "old.txt").read_text(encoding="utf-8"), "old")

    def test_schema_replacement_activation_preserves_tombstones(self) -> None:
        replacement_root = self.root / "replacement-root"
        replacement_root.mkdir()
        (replacement_root / "new-schema.txt").write_text("new", encoding="utf-8")
        (replacement_root / "retry-tombstones.json").write_bytes((self.source / "retry-tombstones.json").read_bytes())
        (self.source / "launch-disabled.json").write_text(json.dumps({"launch_disabled": True, "intake_blocked": True, "active_claims_fenced": True, "effects_reconciled": True}), encoding="utf-8")
        old_archive = self.root / "old-state.zip"
        replaced = self.run_tool("schema_replace", "--state-dir", str(self.source), "--archive-path", str(old_archive), "--replacement", str(replacement_root), "--binary-compatibility-version", "test-v1", "--expected-binary-compatibility-version", "test-v2")
        self.assertEqual(replaced.returncode, 0, replaced.stderr)
        self.assertEqual((self.source / "new-schema.txt").read_text(encoding="utf-8"), "new")
        self.assertTrue(old_archive.is_file())
        self.assertTrue(next(self.root.glob("source.rollback.*")).is_dir())
        with zipfile.ZipFile(old_archive) as archived_old:
            old_manifest = json.loads(archived_old.read(state.MANIFEST_NAME))
        self.assertEqual(old_manifest["binary_compatibility_version"], "test-v1")
        activated_manifest = json.loads((self.source / state.MANIFEST_NAME).read_text(encoding="utf-8"))
        self.assertEqual(activated_manifest["binary_compatibility_version"], "test-v2")

    def test_failed_restore_preserves_old_state(self) -> None:
        self.backup()
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_text("must remain", encoding="utf-8")
        damaged = self.root / "damaged.zip"
        with zipfile.ZipFile(self.archive) as source, zipfile.ZipFile(damaged, "w") as destination:
            for entry in source.infolist():
                destination.writestr(entry, source.read(entry.filename))
            destination.writestr("untracked.txt", "bad")
        result = self.run_tool("restore", "--state-dir", str(target), "--archive-path", str(damaged), "--expected-binary-compatibility-version", "test-v1", "--force")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "must remain")

    def test_schema_replacement_rejects_missing_retry_tombstone_without_activation(self) -> None:
        replacement_root = self.root / "replacement-without-tombstone"
        replacement_root.mkdir()
        (replacement_root / "new-schema.txt").write_text("new", encoding="utf-8")
        replacement_archive = self.root / "replacement-without-tombstone.zip"
        self.backup(replacement_root, replacement_archive)
        (self.source / "launch-disabled.json").write_text(json.dumps({"launch_disabled": True, "intake_blocked": True, "active_claims_fenced": True, "effects_reconciled": True}), encoding="utf-8")
        old_archive = self.root / "old-state.zip"
        rejected = self.run_tool("schema_replace", "--state-dir", str(self.source), "--archive-path", str(old_archive), "--replacement", str(replacement_archive), "--binary-compatibility-version", "test-v1", "--expected-binary-compatibility-version", "test-v1")
        self.assertEqual(self.failure_reason(rejected), "retry_tombstone_rejected")
        self.assertEqual((self.source / "attachments" / "proof.txt").read_text(encoding="utf-8"), "proof")

    def test_binary_compatibility_is_required_and_mismatch_cannot_overwrite_old_state(self) -> None:
        rejected_backup = self.run_tool("backup", "--state-dir", str(self.source), "--archive-path", str(self.archive))
        self.assertEqual(self.failure_reason(rejected_backup), "binary_compatibility_rejected")
        self.backup()
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_text("old", encoding="utf-8")
        mismatch = self.run_tool("restore", "--state-dir", str(target), "--archive-path", str(self.archive), "--expected-binary-compatibility-version", "test-v2", "--force")
        self.assertEqual(self.failure_reason(mismatch), "binary_compatibility_rejected")
        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "old")
        unknown = self.root / "unknown.zip"
        self.rewrite_manifest(unknown, lambda manifest: manifest.update(binary_compatibility_version="unknown"))
        unknown_result = self.run_tool("restore", "--state-dir", str(target), "--archive-path", str(unknown), "--expected-binary-compatibility-version", "test-v1", "--force")
        self.assertEqual(self.failure_reason(unknown_result), "binary_compatibility_rejected")
        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "old")

    def test_duplicate_manifest_record_and_windows_alias_archives_reject_before_activation(self) -> None:
        self.backup()
        duplicate_manifest = self.root / "duplicate-manifest.zip"
        self.rewrite_manifest(duplicate_manifest, lambda manifest: manifest["files"].append(dict(manifest["files"][-1])))
        self.assertEqual(
            self.failure_reason(
                self.run_tool("verify", "--archive-path", str(duplicate_manifest))
            ),
            "archive_validation_rejected",
        )
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_text("old", encoding="utf-8")
        for name, entries, expected in (
            ("ads", [("safe:ads", b"x")], "Unsafe archive entry"),
            ("reserved", [("CON.txt", b"x")], "Unsafe archive entry"),
            ("case-alias", [("proof.txt", b"x"), ("PROOF.TXT", b"y")], "Duplicate or Windows-alias archive entry"),
        ):
            archive = self.root / f"{name}.zip"
            with zipfile.ZipFile(archive, "w") as malicious:
                for entry_name, content in entries:
                    malicious.writestr(entry_name, content)
            result = self.run_tool("restore", "--state-dir", str(target), "--archive-path", str(archive), "--expected-binary-compatibility-version", "test-v1", "--force")
            del expected
            self.assertEqual(self.failure_reason(result), "archive_validation_rejected")
            self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "old")

    def test_cli_success_and_failure_never_echo_secret_bearing_paths(self) -> None:
        secret_source = self.root / "state-root-path-canary-7f93"
        self.source.rename(secret_source)
        secret_archive = self.root / "archive-path-canary-2ac1.zip"
        backup = self.run_tool(
            "backup",
            "--state-dir",
            str(secret_source),
            "--archive-path",
            str(secret_archive),
            "--binary-compatibility-version",
            "test-v1",
        )
        self.assertEqual(backup.returncode, 0, backup.stderr)
        backup_payload = json.loads(backup.stdout.splitlines()[0])
        self.assertTrue(backup_payload["root_identity"].startswith("sha256:"))
        self.assertTrue(backup_payload["archive_identity"].startswith("sha256:"))

        secret_target = self.root / "restore-target-path-canary-c241"
        restored = self.run_tool(
            "restore",
            "--state-dir",
            str(secret_target),
            "--archive-path",
            str(secret_archive),
            "--expected-binary-compatibility-version",
            "test-v1",
        )
        self.assertEqual(restored.returncode, 0, restored.stderr)
        restore_payload = json.loads(restored.stdout.splitlines()[0])
        self.assertTrue(restore_payload["root_identity"].startswith("sha256:"))
        self.assertNotIn("state_root", restore_payload)
        self.assertNotIn("rollback", restore_payload)

        missing = self.root / "missing-archive-path-canary-6e10.zip"
        rejected = self.run_tool("verify", "--archive-path", str(missing))
        self.assertEqual(self.failure_reason(rejected), "archive_validation_rejected")
        blocked_parent = self.root / "ordinary-os-error-canary-91b7"
        blocked_parent.write_text("not a directory", encoding="utf-8")
        os_error = self.run_tool(
            "backup",
            "--state-dir",
            str(secret_source),
            "--archive-path",
            str(blocked_parent / "state.zip"),
            "--binary-compatibility-version",
            "test-v1",
        )
        self.assertEqual(self.failure_reason(os_error), "state_operation_rejected")
        self.assertNotIn("Traceback", os_error.stderr)
        combined = (
            backup.stdout
            + backup.stderr
            + restored.stdout
            + restored.stderr
            + rejected.stdout
            + rejected.stderr
            + os_error.stdout
            + os_error.stderr
        )
        for canary in (
            "state-root-path-canary-7f93",
            "archive-path-canary-2ac1",
            "restore-target-path-canary-c241",
            "missing-archive-path-canary-6e10",
            "ordinary-os-error-canary-91b7",
        ):
            self.assertNotIn(canary, combined)


if __name__ == "__main__":
    unittest.main()
