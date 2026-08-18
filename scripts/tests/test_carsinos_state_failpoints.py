"""EA-105 crash-boundary matrix for state schema replacement."""
from __future__ import annotations

import hashlib
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import zipfile
from contextlib import ExitStack
from unittest import mock
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import carsinos_state as state  # noqa: E402


# Machine-readable inventory: changing or deleting a row changes the proof matrix.
FAILPOINT_CASES = (
    {"name": "archive_source_copy.before", "boundary": "archive_source_copy", "side": "before", "active": "old", "archive": "absent"},
    {"name": "archive_source_copy.after", "boundary": "archive_source_copy", "side": "after", "active": "old", "archive": "absent"},
    {"name": "archive_zip_write.before", "boundary": "archive_zip_write", "side": "before", "active": "old", "archive": "absent"},
    {"name": "archive_zip_write.after", "boundary": "archive_zip_write", "side": "after", "active": "old", "archive": "absent"},
    {"name": "archive_file_sync.before", "boundary": "archive_file_sync", "side": "before", "active": "old", "archive": "absent"},
    {"name": "archive_file_sync.after", "boundary": "archive_file_sync", "side": "after", "active": "old", "archive": "absent"},
    {"name": "archive_publish_rename.before", "boundary": "archive_publication_rename", "side": "before", "active": "old", "archive": "absent"},
    {"name": "archive_publish_rename.after", "boundary": "archive_publication_rename", "side": "after", "active": "old", "archive": "present"},
    {"name": "archive_publish_metadata_sync.before", "boundary": "archive_publication_metadata_sync", "side": "before", "active": "old", "archive": "present"},
    {"name": "archive_publish_metadata_sync.after", "boundary": "archive_publication_metadata_sync", "side": "after", "active": "old", "archive": "present"},
    {"name": "new_root_stage_init.before", "boundary": "new_root_stage_initialization", "side": "before", "active": "old", "archive": "present"},
    {"name": "new_root_stage_init.after", "boundary": "new_root_stage_initialization", "side": "after", "active": "old", "archive": "present"},
    {"name": "candidate_extract_copy.before", "boundary": "extraction_copy", "side": "before", "active": "old", "archive": "present"},
    {"name": "candidate_extract_copy.after", "boundary": "extraction_copy", "side": "after", "active": "old", "archive": "present"},
    {"name": "candidate_validation.before", "boundary": "candidate_validation", "side": "before", "active": "old", "archive": "present"},
    {"name": "candidate_validation.after", "boundary": "candidate_validation", "side": "after", "active": "old", "archive": "present"},
    {"name": "pre_schema_activation", "boundary": "pre_schema_activation", "side": "before", "active": "old", "archive": "present"},
    {"name": "old_root_rollback_rename.before", "boundary": "old_root_rollback_rename", "side": "before", "active": "old", "archive": "present"},
    {"name": "old_root_rollback_rename.after", "boundary": "old_root_rollback_rename", "side": "after", "active": "old", "archive": "present"},
    {"name": "old_root_rollback_metadata_sync.before", "boundary": "old_root_rollback_metadata_sync", "side": "before", "active": "old", "archive": "present"},
    {"name": "old_root_rollback_metadata_sync.after", "boundary": "old_root_rollback_metadata_sync", "side": "after", "active": "old", "archive": "present"},
    {"name": "new_root_activation_rename.before", "boundary": "new_root_activation_rename", "side": "before", "active": "old", "archive": "present"},
    {"name": "new_root_activation_rename.after", "boundary": "new_root_activation_rename", "side": "after", "active": "old", "archive": "present"},
    {"name": "new_root_activation_metadata_sync.before", "boundary": "new_root_activation_metadata_sync", "side": "before", "active": "old", "archive": "present"},
    {"name": "new_root_activation_metadata_sync.after", "boundary": "new_root_activation_metadata_sync", "side": "after", "active": "old", "archive": "present"},
    {"name": "post_activation_verification.before", "boundary": "post_activation_verification", "side": "before", "active": "old", "archive": "present"},
    {"name": "post_activation_verification.after", "boundary": "post_activation_verification", "side": "after", "active": "old", "archive": "present"},
    {"name": "post_activation_finalization.before", "boundary": "post_activation_finalization", "side": "before", "active": "old", "archive": "present"},
    {"name": "post_activation_finalization.after", "boundary": "post_activation_finalization", "side": "after", "active": "new", "archive": "present"},
)


def file_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def snapshot_tree(root: Path) -> dict[str, dict[str, int | str]]:
    return {
        path.relative_to(root).as_posix(): {"size": path.stat().st_size, "sha256": file_hash(path)}
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


class CarsinosStateFailpointTests(unittest.TestCase):
    maxDiff = None

    def make_fixture(self, root: Path) -> tuple[Path, Path, Path]:
        active = root / "source"
        active.mkdir()
        (active / "attachments").mkdir()
        (active / "attachments" / "proof.bin").write_bytes(b"old-proof\x00\xff")
        (active / "receipt-anchor.json").write_text(
            json.dumps({"state_root_generation": "old-generation", "key_id": "old-key"}),
            encoding="utf-8",
        )
        (active / "retry-tombstones.json").write_bytes(b'{"effect-1":"blocked"}\n')
        (active / "launch-disabled.json").write_text(
            json.dumps({
                "launch_disabled": True,
                "intake_blocked": True,
                "active_claims_fenced": True,
                "effects_reconciled": True,
            }),
            encoding="utf-8",
        )
        database = sqlite3.connect(active / "carsinos.db")
        database.execute("PRAGMA foreign_keys = ON")
        database.execute("CREATE TABLE receipts (id INTEGER PRIMARY KEY, body BLOB NOT NULL)")
        database.execute("INSERT INTO receipts(body) VALUES (?)", (b"receipt-old",))
        database.execute("PRAGMA user_version = 7")
        database.commit()
        database.close()

        replacement = root / "replacement"
        replacement.mkdir()
        (replacement / "new-schema.txt").write_bytes(b"new-schema-v2")
        (replacement / "retry-tombstones.json").write_bytes((active / "retry-tombstones.json").read_bytes())
        database = sqlite3.connect(replacement / "carsinos.db")
        database.execute("CREATE TABLE delegations (id TEXT PRIMARY KEY, revision INTEGER NOT NULL)")
        database.execute("PRAGMA user_version = 8")
        database.commit()
        database.close()
        return active, replacement, root / "old-state.zip"

    def run_replace(self, active: Path, replacement: Path, archive: Path, *, failpoint: str, test_mode: bool = True) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        environment.pop(state.TEST_MODE_ENV, None)
        environment.pop(state.TEST_FAILPOINT_ENV, None)
        environment[state.TEST_FAILPOINT_ENV] = failpoint
        if test_mode:
            environment[state.TEST_MODE_ENV] = "1"
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPTS / "carsinos_state.py"),
                "schema_replace",
                "--state-dir",
                str(active),
                "--archive-path",
                str(archive),
                "--replacement",
                str(replacement),
                "--binary-compatibility-version",
                "test-v1",
                "--expected-binary-compatibility-version",
                "test-v2",
            ],
            cwd=Path.cwd(),
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def assert_archive_snapshot(self, archive: Path, old_snapshot: dict[str, dict[str, int | str]]) -> None:
        manifest = state.verify_archive(archive)
        archived = {
            record["path"]: {"size": record["size_bytes"], "sha256": record["sha256"]}
            for record in manifest["files"]
        }
        self.assertEqual(archived, old_snapshot)

    def assert_no_temporary_residue(self, root: Path) -> None:
        residue = [
            path.relative_to(root).as_posix()
            for path in root.rglob("*")
            if ".stage." in path.name or path.name.startswith(".source.replacement.")
        ]
        self.assertEqual(residue, [])

    def test_all_named_failpoints_preserve_an_exact_valid_state(self) -> None:
        self.assertEqual({case["name"] for case in FAILPOINT_CASES}, state.FAILPOINTS)
        self.assertEqual(len(FAILPOINT_CASES), 29)
        for case in FAILPOINT_CASES:
            with self.subTest(failpoint=case["name"]), tempfile.TemporaryDirectory(
                prefix="carsinos-ea105-case-", dir=Path.cwd()
            ) as temporary:
                root = Path(temporary)
                active, replacement, archive = self.make_fixture(root)
                old_snapshot = snapshot_tree(active)
                replacement_snapshot = snapshot_tree(replacement)

                result = self.run_replace(active, replacement, archive, failpoint=case["name"])

                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(
                    json.loads(result.stderr.splitlines()[0])["reason"],
                    "interrupted_for_test",
                )
                self.assertNotIn(case["name"], result.stderr)
                self.assertEqual(snapshot_tree(replacement), replacement_snapshot)
                rollbacks = list(root.glob("source.rollback.*"))
                if case["active"] == "old":
                    self.assertEqual(snapshot_tree(active), old_snapshot)
                    self.assertEqual(rollbacks, [])
                    self.assertFalse((active / "new-schema.txt").exists())
                else:
                    self.assertEqual(len(rollbacks), 1)
                    self.assertEqual(snapshot_tree(rollbacks[0]), old_snapshot)
                    manifest = state.verify_expanded(active)
                    new_snapshot = snapshot_tree(active)
                    manifest_snapshot = {
                        record["path"]: {"size": record["size_bytes"], "sha256": record["sha256"]}
                        for record in manifest["files"]
                    }
                    self.assertEqual(
                        {path: metadata for path, metadata in new_snapshot.items() if path != state.MANIFEST_NAME},
                        manifest_snapshot,
                    )
                    self.assertIn(state.MANIFEST_NAME, new_snapshot)
                    self.assertEqual(manifest["binary_compatibility_version"], "test-v2")
                    self.assertEqual((active / "new-schema.txt").read_bytes(), b"new-schema-v2")
                    self.assertEqual(
                        (active / "retry-tombstones.json").read_bytes(),
                        (rollbacks[0] / "retry-tombstones.json").read_bytes(),
                    )
                self.assertEqual(archive.exists(), case["archive"] == "present")
                if archive.exists():
                    self.assert_archive_snapshot(archive, old_snapshot)
                self.assert_no_temporary_residue(root)

    def test_unknown_failpoint_is_rejected_before_state_changes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="carsinos-ea105-unknown-", dir=Path.cwd()) as temporary:
            root = Path(temporary)
            active, replacement, archive = self.make_fixture(root)
            old_snapshot = snapshot_tree(active)
            result = self.run_replace(active, replacement, archive, failpoint="not-a-real-boundary")
            self.assertEqual(result.returncode, 2)
            self.assertEqual(
                json.loads(result.stderr.splitlines()[0])["reason"],
                "interrupted_for_test",
            )
            self.assertNotIn("not-a-real-boundary", result.stderr)
            self.assertEqual(snapshot_tree(active), old_snapshot)
            self.assertFalse(archive.exists())
            self.assertEqual(list(root.glob("source.rollback.*")), [])
            self.assert_no_temporary_residue(root)

    def test_failpoint_environment_is_inert_without_test_mode(self) -> None:
        with tempfile.TemporaryDirectory(prefix="carsinos-ea105-inert-", dir=Path.cwd()) as temporary:
            root = Path(temporary)
            active, replacement, archive = self.make_fixture(root)
            old_snapshot = snapshot_tree(active)
            result = self.run_replace(
                active,
                replacement,
                archive,
                failpoint="old_root_rollback_rename.after",
                test_mode=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            rollback = next(root.glob("source.rollback.*"))
            self.assertEqual(snapshot_tree(rollback), old_snapshot)
            manifest = state.verify_expanded(active)
            self.assertEqual(manifest["binary_compatibility_version"], "test-v2")
            self.assert_archive_snapshot(archive, old_snapshot)
            self.assert_no_temporary_residue(root)

    def test_existing_archive_is_never_overwritten(self) -> None:
        with tempfile.TemporaryDirectory(prefix="carsinos-ea105-existing-", dir=Path.cwd()) as temporary:
            root = Path(temporary)
            active, replacement, archive = self.make_fixture(root)
            old_snapshot = snapshot_tree(active)
            archive.write_bytes(b"pre-existing-archive")
            environment = os.environ.copy()
            environment.pop(state.TEST_MODE_ENV, None)
            environment.pop(state.TEST_FAILPOINT_ENV, None)
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPTS / "carsinos_state.py"),
                    "schema_replace",
                    "--state-dir",
                    str(active),
                    "--archive-path",
                    str(archive),
                    "--replacement",
                    str(replacement),
                    "--binary-compatibility-version",
                    "test-v1",
                    "--expected-binary-compatibility-version",
                    "test-v2",
                ],
                cwd=Path.cwd(),
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 2)
            self.assertEqual(
                json.loads(result.stderr.splitlines()[0])["reason"],
                "archive_validation_rejected",
            )
            self.assertEqual(archive.read_bytes(), b"pre-existing-archive")
            self.assertEqual(snapshot_tree(active), old_snapshot)
            self.assertEqual(list(root.glob("source.rollback.*")), [])
            self.assert_no_temporary_residue(root)

    def test_archive_publication_race_cannot_replace_concurrent_winner(self) -> None:
        with tempfile.TemporaryDirectory(prefix="carsinos-ea105-race-", dir=Path.cwd()) as temporary:
            root = Path(temporary)
            active, _, archive = self.make_fixture(root)
            active_snapshot = snapshot_tree(active)

            def create_concurrent_winner(name: str) -> None:
                if name == "archive_publish_rename.before":
                    archive.write_bytes(b"concurrent-winner")

            with mock.patch.object(state, "failpoint", side_effect=create_concurrent_winner):
                with self.assertRaisesRegex(state.StateToolError, "Backup archive already exists"):
                    state.make_archive(active, archive, "test-v1")

            self.assertEqual(archive.read_bytes(), b"concurrent-winner")
            self.assertEqual(snapshot_tree(active), active_snapshot)
            self.assert_no_temporary_residue(root)

    def test_archive_resource_limits_reject_before_extracting_any_file(self) -> None:
        with tempfile.TemporaryDirectory(prefix="carsinos-ea105-limits-", dir=Path.cwd()) as temporary:
            root = Path(temporary)

            def assert_rejected(archive: Path, patches: tuple[object, ...], expected: str) -> None:
                destination = root / f"extract-{archive.stem}"
                destination.mkdir()
                with ExitStack() as stack:
                    for patch in patches:
                        stack.enter_context(patch)
                    with self.assertRaisesRegex(state.StateToolError, expected):
                        state.extract_safe(archive, destination)
                self.assertEqual(snapshot_tree(destination), {})

            member_archive = root / "members.zip"
            with zipfile.ZipFile(member_archive, "w") as destination:
                for index in range(3):
                    destination.writestr(f"file-{index}.txt", b"x")
            assert_rejected(
                member_archive,
                (mock.patch.object(state, "MAX_ARCHIVE_MEMBERS", 2),),
                "member count exceeds limit",
            )

            entry_archive = root / "entry.zip"
            with zipfile.ZipFile(entry_archive, "w") as destination:
                destination.writestr("large.bin", b"12345678")
            assert_rejected(
                entry_archive,
                (mock.patch.object(state, "MAX_ARCHIVE_ENTRY_UNCOMPRESSED_BYTES", 7),),
                "entry exceeds uncompressed size limit",
            )

            total_archive = root / "total.zip"
            with zipfile.ZipFile(total_archive, "w") as destination:
                destination.writestr("first.bin", b"123456")
                destination.writestr("second.bin", b"123456")
            assert_rejected(
                total_archive,
                (mock.patch.object(state, "MAX_ARCHIVE_TOTAL_UNCOMPRESSED_BYTES", 10),),
                "total uncompressed size exceeds limit",
            )

            ratio_archive = root / "ratio.zip"
            with zipfile.ZipFile(ratio_archive, "w", compression=zipfile.ZIP_DEFLATED) as destination:
                destination.writestr("compressed.bin", b"0" * 4096)
            assert_rejected(
                ratio_archive,
                (mock.patch.object(state, "MAX_ARCHIVE_COMPRESSION_RATIO", 2),),
                "declared compression ratio limit",
            )

            impossible = zipfile.ZipInfo("impossible.bin")
            impossible.file_size = 1
            impossible.compress_size = 0
            with self.assertRaisesRegex(state.StateToolError, "impossible declared compression size"):
                state.validate_archive_resources([impossible])


if __name__ == "__main__":
    unittest.main()
