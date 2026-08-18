"""EA-111 positive-control and state-tool raw-byte secrecy gates."""
from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import carsinos_state as state  # noqa: E402
from sensitive_byte_scan import (  # noqa: E402
    SensitiveByteScanError,
    make_runtime_canary,
    require_clean,
    require_detected_labels,
    scan_tree,
    scan_zip,
)


class SensitiveByteScanTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="carsinos-ea111-", dir=Path.cwd())
        self.root = Path(self.temp.name)
        self.canary = make_runtime_canary("ea111")
        self.labels = {f"ea111:{name}" for name in self.canary.variants()}

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_poisoned_corpus(self) -> tuple[Path, Path]:
        corpus = self.root / "poisoned"
        corpus.mkdir()
        locations = (
            "carsinos.db",
            "carsinos.db-wal",
            "carsinos.db-shm",
            "logs/diagnostics.log",
            "outbox/notification-capture.ndjson",
            "notifications/fixture-capture.ndjson",
            "exports/export.ndjson",
            "urls/generated-url.txt",
        )
        for index, (_, variant) in enumerate(sorted(self.canary.variants().items())):
            target = corpus / locations[index % len(locations)]
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("ab") as output:
                output.write(variant + b"\n")
        manifest = {"generated_url": self.canary.variants()["plus_query"].decode("ascii")}
        (corpus / "backup-manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        encoded_member_name = self.canary.variants()["hex_upper"].decode("ascii")
        member_name_probe = corpus / "member-names" / f"{encoded_member_name}.bin"
        member_name_probe.parent.mkdir()
        member_name_probe.write_bytes(b"redacted-content")
        archive = self.root / "poisoned.zip"
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_STORED) as output:
            for path in sorted(corpus.rglob("*")):
                if path.is_file():
                    output.write(path, path.relative_to(corpus).as_posix())
        return corpus, archive

    def test_positive_control_detects_every_declared_variant_on_every_representative_surface(self) -> None:
        corpus, archive = self.write_poisoned_corpus()
        tree_findings = scan_tree(corpus, (self.canary,))
        archive_findings = scan_zip(archive, (self.canary,))
        require_detected_labels(self.labels, tree_findings)
        require_detected_labels(self.labels, archive_findings)
        surfaces = {finding.surface for finding in tree_findings + archive_findings}
        self.assertTrue({
            "state_database",
            "state_logs",
            "state_outbox",
            "state_notifications",
            "state_exports",
            "state_urls",
            "state_manifest",
            "state_path",
            "archive_raw",
            "archive_member",
            "archive_member_name",
        }.issubset(surfaces))
        self.assertTrue(all(finding.locator.startswith("sha256:") for finding in tree_findings + archive_findings))

        with self.assertRaises(SensitiveByteScanError) as failure:
            require_clean(tree_findings + archive_findings)
        message = str(failure.exception)
        self.assertNotIn(self.canary.value, message)
        for variant in self.canary.variants().values():
            try:
                rendered = variant.decode("ascii")
            except UnicodeDecodeError:
                continue
            self.assertNotIn(rendered, message)

    def test_each_variant_removal_makes_the_positive_control_gate_fail(self) -> None:
        for name, variant in self.canary.variants().items():
            with self.subTest(variant=name):
                root = self.root / f"mutation-{name}"
                root.mkdir()
                target = root / "outbox" / "capture.bin"
                target.parent.mkdir()
                target.write_bytes(variant)
                label = f"ea111:{name}"
                require_detected_labels({label}, scan_tree(root, (self.canary,)))
                target.write_bytes(b"redacted")
                with self.assertRaisesRegex(SensitiveByteScanError, "missed declared canary variants"):
                    require_detected_labels({label}, scan_tree(root, (self.canary,)))

    def test_safe_corpus_and_protected_store_exclusion_have_zero_hits(self) -> None:
        safe = self.root / "safe"
        (safe / "logs").mkdir(parents=True)
        (safe / "outbox").mkdir()
        (safe / "exports").mkdir()
        (safe / "secrets").mkdir()
        (safe / "carsinos.db").write_bytes(b"SQLite format 3\x00safe")
        (safe / "carsinos.db-wal").write_bytes(b"safe")
        (safe / "carsinos.db-shm").write_bytes(b"safe")
        (safe / "logs" / "diagnostics.log").write_text("redacted", encoding="utf-8")
        (safe / "outbox" / "notification-capture.ndjson").write_text("redacted", encoding="utf-8")
        (safe / "exports" / "state.json").write_text("redacted", encoding="utf-8")
        # This is an OS-protected-store stand-in and is the sole explicit exclusion.
        (safe / "secrets" / "token").write_text(self.canary.value, encoding="utf-8")
        archive = self.root / "safe.zip"
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_STORED) as output:
            for path in sorted(safe.rglob("*")):
                if path.is_file() and "secrets" not in path.parts:
                    output.write(path, path.relative_to(safe).as_posix())
        tree_findings = scan_tree(safe, (self.canary,))
        archive_findings = scan_zip(archive, (self.canary,))
        self.assertEqual(tree_findings, [])
        self.assertEqual(archive_findings, [])
        require_clean(tree_findings + archive_findings)

    def test_state_tool_test_registry_is_inert_by_default_and_blocks_all_boundaries(self) -> None:
        source = self.root / "source"
        source.mkdir()
        (source / "attachments").mkdir()
        (source / "attachments" / "proof.txt").write_text("safe", encoding="utf-8")
        database = sqlite3.connect(source / "carsinos.db")
        database.execute("CREATE TABLE sample (id INTEGER PRIMARY KEY)")
        database.commit()
        database.close()
        registry = self.root / "runtime-canary-registry.json"
        registry.write_text(json.dumps([{"identifier": self.canary.identifier, "value": self.canary.value}]), encoding="utf-8")
        environment = os.environ.copy()
        environment[state.TEST_MODE_ENV] = "1"
        environment[state.TEST_CANARY_REGISTRY_ENV] = str(registry)
        archive = self.root / "state.zip"
        command = [sys.executable, str(SCRIPTS / "carsinos_state.py")]

        (source / "logs").mkdir()
        (source / "logs" / "diagnostics.log").write_bytes(self.canary.variants()["utf8"])
        blocked_backup = subprocess.run(
            [*command, "backup", "--state-dir", str(source), "--archive-path", str(archive), "--binary-compatibility-version", "test-v1"],
            env=environment, text=True, capture_output=True, check=False,
        )
        self.assertNotEqual(blocked_backup.returncode, 0)
        self.assertEqual(
            json.loads(blocked_backup.stderr.splitlines()[0])["reason"],
            "sensitive_data_detected",
        )
        self.assertFalse(archive.exists())

        (source / "logs" / "diagnostics.log").write_text("redacted", encoding="utf-8")
        clean_backup = subprocess.run(
            [*command, "backup", "--state-dir", str(source), "--archive-path", str(archive), "--binary-compatibility-version", "test-v1"],
            text=True, capture_output=True, check=False,
        )
        self.assertEqual(clean_backup.returncode, 0, clean_backup.stderr)
        with zipfile.ZipFile(archive, "a") as output:
            output.comment = self.canary.variants()["base64url_padded"]
        blocked_verify = subprocess.run([*command, "verify", "--archive-path", str(archive)], env=environment, text=True, capture_output=True, check=False)
        self.assertNotEqual(blocked_verify.returncode, 0)
        self.assertEqual(
            json.loads(blocked_verify.stderr.splitlines()[0])["reason"],
            "sensitive_data_detected",
        )
        target = self.root / "target"
        target.mkdir()
        (target / "old.txt").write_text("must remain", encoding="utf-8")
        blocked_restore = subprocess.run(
            [*command, "restore", "--state-dir", str(target), "--archive-path", str(archive), "--expected-binary-compatibility-version", "test-v1", "--force"],
            env=environment, text=True, capture_output=True, check=False,
        )
        self.assertNotEqual(blocked_restore.returncode, 0)
        self.assertEqual((target / "old.txt").read_text(encoding="utf-8"), "must remain")

        replacement = self.root / "replacement"
        replacement.mkdir()
        (replacement / "retry-tombstones.json").write_text("{}", encoding="utf-8")
        (replacement / "urls.txt").write_bytes(self.canary.variants()["percent_upper"])
        (source / "launch-disabled.json").write_text(json.dumps({"launch_disabled": True, "intake_blocked": True, "active_claims_fenced": True, "effects_reconciled": True}), encoding="utf-8")
        old_archive = self.root / "old-state.zip"
        blocked_replace = subprocess.run(
            [*command, "schema_replace", "--state-dir", str(source), "--archive-path", str(old_archive), "--replacement", str(replacement), "--binary-compatibility-version", "test-v1", "--expected-binary-compatibility-version", "test-v2"],
            env=environment, text=True, capture_output=True, check=False,
        )
        self.assertNotEqual(blocked_replace.returncode, 0)
        self.assertEqual((source / "attachments" / "proof.txt").read_text(encoding="utf-8"), "safe")


if __name__ == "__main__":
    unittest.main()
