#!/usr/bin/env python3
"""CarsinOS offline state archive, verification, restore, and schema replacement.

This tool deliberately has no reset or erase action.  It only activates a fully
verified root after an offline/quiescent check and keeps a rollback root.
"""
from __future__ import annotations

import argparse
import ctypes
import errno
import hashlib
import json
import os
import shutil
import sqlite3
import stat
import subprocess
import sys
import tempfile
import uuid
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any, Callable, Iterator

from sensitive_byte_scan import Canary, SensitiveByteScanError, require_clean, scan_tree, scan_zip

ARCHIVE_SCHEMA = "carsinos.state-archive.v2"
TOOL_SCHEMA = "carsinos.state-tool.v1"
EXCLUDED_TOP_LEVEL = frozenset({
    "cargo-target", "npm-cache", "tmp", "pids", "logs", "locks",
    "launcher-scripts", "codex-bridge", "codex-bridge-workspaces", "secrets",
})
MANIFEST_NAME = "backup-manifest.json"
PID_FILES = ("gateway.pid", "pids/gateway.pid", "pids/host.pid")
TOMBSTONE_TOKENS = ("tombstone", "retry-tomb", "retry_tomb")
RECEIPT_ANCHOR_TABLE = "execass_receipt_anchor_state"
RECEIPT_TABLE = "execass_receipts"
WINDOWS_RESERVED_NAMES = frozenset({"con", "prn", "aux", "nul", *(f"com{i}" for i in range(1, 10)), *(f"lpt{i}" for i in range(1, 10))})
# The local product may carry large attachment trees, but restore must reject
# metadata that could expand into an unbounded local resource-exhaustion load.
MAX_ARCHIVE_MEMBERS = 100_000
MAX_ARCHIVE_ENTRY_UNCOMPRESSED_BYTES = 16 * 1024 * 1024 * 1024
MAX_ARCHIVE_TOTAL_UNCOMPRESSED_BYTES = 128 * 1024 * 1024 * 1024
MAX_ARCHIVE_COMPRESSION_RATIO = 10_000
TEST_MODE_ENV = "CARSINOS_STATE_TOOL_TEST_MODE"
TEST_FAILPOINT_ENV = "CARSINOS_STATE_TOOL_TEST_FAILPOINT"
TEST_CANARY_REGISTRY_ENV = "CARSINOS_STATE_TOOL_TEST_CANARY_REGISTRY"
FAILPOINTS = frozenset({
    "archive_source_copy.before",
    "archive_source_copy.after",
    "archive_zip_write.before",
    "archive_zip_write.after",
    "archive_file_sync.before",
    "archive_file_sync.after",
    "archive_publish_rename.before",
    "archive_publish_rename.after",
    "archive_publish_metadata_sync.before",
    "archive_publish_metadata_sync.after",
    "new_root_stage_init.before",
    "new_root_stage_init.after",
    "candidate_extract_copy.before",
    "candidate_extract_copy.after",
    "candidate_validation.before",
    "candidate_validation.after",
    "pre_schema_activation",
    "old_root_rollback_rename.before",
    "old_root_rollback_rename.after",
    "old_root_rollback_metadata_sync.before",
    "old_root_rollback_metadata_sync.after",
    "new_root_activation_rename.before",
    "new_root_activation_rename.after",
    "new_root_activation_metadata_sync.before",
    "new_root_activation_metadata_sync.after",
    "post_activation_verification.before",
    "post_activation_verification.after",
    "post_activation_finalization.before",
    "post_activation_finalization.after",
})
_active_test_failpoint: str | None = None


class StateToolError(RuntimeError):
    pass


class SafeArgumentParser(argparse.ArgumentParser):
    """Reject malformed CLI input without reflecting secret-bearing arguments."""

    def error(self, message: str) -> None:
        del message
        self.exit(
            2,
            json.dumps(
                {"ok": False, "action": "unknown", "reason": "invalid_arguments"},
                sort_keys=True,
            )
            + "\n",
        )


def registered_test_canaries() -> tuple[Canary, ...]:
    """Load an additive, test-only scanner registry; production stays inert.

    The registry file is intentionally not a CLI option.  Its environment hook
    is honored only together with the existing explicit test-mode gate and can
    only add fail-closed scanning to normal archive verification.
    """
    if os.environ.get(TEST_MODE_ENV) != "1":
        return ()
    registry_value = os.environ.get(TEST_CANARY_REGISTRY_ENV)
    if not registry_value:
        return ()
    registry = canonical(registry_value)
    try:
        payload = json.loads(registry.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise StateToolError("Test canary registry is unreadable or invalid.") from error
    if not isinstance(payload, list):
        raise StateToolError("Test canary registry is invalid.")
    canaries: list[Canary] = []
    for item in payload:
        if not isinstance(item, dict):
            raise StateToolError("Test canary registry is invalid.")
        identifier = item.get("identifier")
        value = item.get("value")
        if not isinstance(identifier, str) or not isinstance(value, str):
            raise StateToolError("Test canary registry is invalid.")
        canaries.append(Canary(identifier, value))
    if not canaries:
        raise StateToolError("Test canary registry is empty.")
    return tuple(canaries)


def require_clean_canaries(
    *, root: Path | None = None, archive: Path | None = None, canaries: tuple[Canary, ...] = ()
) -> None:
    if not canaries:
        return
    try:
        findings = []
        if root is not None:
            findings.extend(scan_tree(root, canaries))
        if archive is not None:
            findings.extend(scan_zip(archive, canaries))
        require_clean(findings)
    except SensitiveByteScanError as error:
        raise StateToolError(str(error)) from error


def configure_test_failpoint() -> None:
    """Enable one named failure only under the explicit test-only environment gate."""
    global _active_test_failpoint
    _active_test_failpoint = None
    if os.environ.get(TEST_MODE_ENV) != "1":
        return
    requested = os.environ.get(TEST_FAILPOINT_ENV)
    if requested is None:
        return
    if requested not in FAILPOINTS:
        raise StateToolError(f"Unknown state-tool test failpoint: {requested}")
    _active_test_failpoint = requested


def failpoint(name: str) -> None:
    if _active_test_failpoint == name:
        raise StateToolError(f"Injected state-tool test failure: {name}")


def canonical(path: str | Path, base: Path | None = None) -> Path:
    value = Path(path).expanduser()
    if not value.is_absolute():
        value = (base or Path.cwd()) / value
    return value.resolve(strict=False)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_path_identity(path: Path) -> str:
    """Return a diagnostic identity without disclosing the configured path."""
    try:
        resolved = canonical(path)
    except Exception:
        resolved = Path(path)
    normalized = os.path.normcase(str(resolved)).encode("utf-8", "surrogatepass")
    return "sha256:" + hashlib.sha256(b"carsinos.state-path.v1\0" + normalized).hexdigest()


def safe_error_category(error: Exception) -> str:
    """Collapse detailed internal failures into a bounded non-echoing CLI category."""
    message = str(error).casefold()
    if "injected state-tool test failure" in message or "state-tool test failpoint" in message:
        return "interrupted_for_test"
    if "sensitive-byte canary detected" in message:
        return "sensitive_data_detected"
    if "receipt" in message:
        return "receipt_integrity_rejected"
    if "binary compatibility" in message:
        return "binary_compatibility_rejected"
    if any(token in message for token in ("running", "live pid", "in use", "launch-disabled", "quiescent")):
        return "state_not_quiescent"
    if any(token in message for token in ("archive", "backup", "manifest", "compression", "checksum", "declared size")):
        return "archive_validation_rejected"
    if "tombstone" in message:
        return "retry_tombstone_rejected"
    if any(token in message for token in ("path", "directory", "state target", "state root", "replacement", "symlink", "reparse")):
        return "state_path_rejected"
    if "schema" in message or "sqlite" in message:
        return "schema_validation_rejected"
    return "state_operation_rejected"


def is_lower_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def safe_relative(value: str) -> PurePosixPath:
    raw = value.replace("\\", "/")
    if not raw or raw.startswith("/") or raw.startswith("//"):
        raise StateToolError(f"Unsafe archive entry: {value}")
    parts = raw.split("/")
    if not parts or any(not part or part in {".", ".."} or ":" in part for part in parts):
        raise StateToolError(f"Unsafe archive entry: {value}")
    for part in parts:
        normalized = part.rstrip(" .")
        if not normalized:
            raise StateToolError(f"Unsafe archive entry: {value}")
        if normalized.split(".", 1)[0].casefold() in WINDOWS_RESERVED_NAMES:
            raise StateToolError(f"Unsafe archive entry: {value}")
    path = PurePosixPath(*parts)
    return path


def windows_path_alias(relative: PurePosixPath) -> str:
    """The filename identity Windows uses after case and trailing-dot normalization."""
    return "/".join(part.rstrip(" .").casefold() for part in relative.parts)


def valid_binary_compatibility(version: Any) -> bool:
    return isinstance(version, str) and bool(version.strip()) and version.strip().casefold() != "unknown"


def require_binary_compatibility(version: Any, *, label: str) -> str:
    if not valid_binary_compatibility(version):
        raise StateToolError(f"{label} must be explicit and cannot be empty or unknown.")
    return version.strip()


def paths_overlap(first: Path, second: Path) -> bool:
    try:
        first.relative_to(second)
        return True
    except ValueError:
        try:
            second.relative_to(first)
            return True
        except ValueError:
            return False


def assert_safe_state_target(state: Path, repo: Path) -> None:
    filesystem_root = Path(state.anchor).resolve(strict=False)
    if state == filesystem_root or state == repo:
        raise StateToolError("State target cannot be a filesystem root or the repository root.")


def is_included(relative: PurePosixPath) -> bool:
    return bool(relative.parts) and relative.parts[0] not in EXCLUDED_TOP_LEVEL


def assert_not_reparse(path: Path) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        return
    if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise StateToolError(f"Symlink or reparse point is not allowed: {path}")


def walk_files(root: Path, *, include_manifest: bool = False) -> Iterator[tuple[PurePosixPath, Path]]:
    assert_not_reparse(root)
    for current, directories, files in os.walk(root, topdown=True, followlinks=False):
        current_path = Path(current)
        safe_directories: list[str] = []
        for name in directories:
            candidate = current_path / name
            assert_not_reparse(candidate)
            relative = PurePosixPath(candidate.relative_to(root).as_posix())
            if is_included(relative):
                safe_directories.append(name)
        directories[:] = safe_directories
        for name in files:
            candidate = current_path / name
            assert_not_reparse(candidate)
            relative = PurePosixPath(candidate.relative_to(root).as_posix())
            if not is_included(relative):
                continue
            if not include_manifest and relative.as_posix() == MANIFEST_NAME:
                continue
            yield relative, candidate


def read_json(path: Path) -> Any | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None


def sqlite_metadata(root: Path) -> list[dict[str, Any]]:
    metadata: list[dict[str, Any]] = []
    for relative, path in walk_files(root):
        if path.suffix.lower() not in {".db", ".sqlite", ".sqlite3"}:
            continue
        try:
            with path.open("rb") as source:
                if source.read(16) != b"SQLite format 3\x00":
                    continue
            connection = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
            try:
                version = connection.execute("PRAGMA user_version").fetchone()[0]
                application_id = connection.execute("PRAGMA application_id").fetchone()[0]
            finally:
                connection.close()
            metadata.append({"path": relative.as_posix(), "user_version": version, "application_id": application_id})
        except (OSError, sqlite3.Error) as error:
            raise StateToolError(f"Cannot read SQLite schema metadata for {relative}: {error}") from error
    return metadata


def receipt_metadata(root: Path) -> list[dict[str, Any]]:
    """Read non-secret receipt high-water metadata from the authoritative DB.

    External anchor/key files are deliberately outside the swappable state root
    and are never archived.  Legacy in-root JSON marker names are ordinary files
    and cannot act as receipt-integrity authority.
    """
    result: list[dict[str, Any]] = []
    for relative, path in walk_files(root):
        if path.suffix.lower() not in {".db", ".sqlite", ".sqlite3"}:
            continue
        try:
            with path.open("rb") as source:
                if source.read(16) != b"SQLite format 3\x00":
                    continue
            connection = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
            try:
                table_exists = connection.execute(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=? LIMIT 1",
                    (RECEIPT_ANCHOR_TABLE,),
                ).fetchone()
                if table_exists is None:
                    continue
                receipt_table_exists = connection.execute(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=? LIMIT 1",
                    (RECEIPT_TABLE,),
                ).fetchone()
                receipt_rows = (
                    connection.execute(f"SELECT COUNT(*) FROM {RECEIPT_TABLE}").fetchone()[0]
                    if receipt_table_exists is not None else 0
                )
                row = connection.execute(
                    f"""
                    SELECT root_identity,state_root_generation,anchor_generation,status,
                           receipt_count,receipt_head_digest,key_id,key_generation,transaction_id,
                           external_receipt_digest
                    FROM {RECEIPT_ANCHOR_TABLE}
                    ORDER BY anchor_generation DESC LIMIT 1
                    """
                ).fetchone()
            finally:
                connection.close()
        except sqlite3.Error as error:
            raise StateToolError(f"Cannot read receipt-integrity metadata for {relative}: {error}") from error
        if row is None:
            if receipt_rows:
                raise StateToolError(
                    f"ExecAss receipts exist without receipt-integrity anchor state: {relative}"
                )
            continue
        (root_identity, state_generation, anchor_generation, status, receipt_count,
         receipt_head, key_id, key_generation, transaction_id, receipt_digest) = row
        if (not isinstance(root_identity, str) or
                not root_identity.startswith("sha256:") or
                not is_lower_sha256(root_identity.removeprefix("sha256:")) or
                not isinstance(state_generation, int) or state_generation <= 0 or
                not isinstance(anchor_generation, int) or anchor_generation <= 0 or
                status not in {"prepared", "finalized", "quarantined"} or
                not isinstance(receipt_count, int) or receipt_count < 0 or
                (receipt_count == 0) != (receipt_head is None) or
                (receipt_head is not None and not is_lower_sha256(receipt_head)) or
                not isinstance(key_id, str) or not key_id or
                not isinstance(key_generation, int) or key_generation <= 0 or
                not isinstance(transaction_id, str) or not transaction_id or
                not is_lower_sha256(receipt_digest)):
            raise StateToolError(f"Receipt-integrity metadata is invalid: {relative}")
        result.append({
            "path": relative.as_posix(),
            "root_identity": root_identity,
            "state_root_generation": state_generation,
            "anchor_generation": anchor_generation,
            "status": status,
            "receipt_count": receipt_count,
            "receipt_head_digest": receipt_head,
            "key_id": key_id,
            "key_generation": key_generation,
            "transaction_id": transaction_id,
            "external_receipt_digest": receipt_digest,
        })
    return sorted(result, key=lambda item: item["path"])


def tombstones(root: Path) -> list[dict[str, str]]:
    values = []
    for relative, path in walk_files(root):
        lowered = relative.as_posix().lower()
        if any(token in lowered for token in TOMBSTONE_TOKENS):
            values.append({"path": relative.as_posix(), "sha256": sha256(path)})
    return sorted(values, key=lambda item: item["path"])


def make_manifest(root: Path, binary_compatibility_version: str) -> dict[str, Any]:
    records = [{"path": relative.as_posix(), "size_bytes": path.stat().st_size, "sha256": sha256(path)}
               for relative, path in walk_files(root)]
    records.sort(key=lambda item: item["path"])
    return {
        "schema": ARCHIVE_SCHEMA,
        "tool_schema": TOOL_SCHEMA,
        "product": "CarsinOS",
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "source_state_root": str(root),
        "source_state_root_generation": next((item["state_root_generation"] for item in receipt_metadata(root)), None),
        "binary_compatibility_version": binary_compatibility_version,
        "excluded_top_level": sorted(EXCLUDED_TOP_LEVEL),
        "secret_references": [{"path": "secrets", "disposition": "excluded_mandatory_reauthentication"}],
        "retry_tombstones": {"present": bool(tombstones(root)), "files": tombstones(root)},
        "receipt_metadata": receipt_metadata(root),
        "sqlite_databases": sqlite_metadata(root),
        "files": records,
    }


def write_manifest(root: Path, manifest: dict[str, Any]) -> None:
    (root / MANIFEST_NAME).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def assert_offline(root: Path) -> None:
    for relative in PID_FILES:
        pid_file = root / relative
        if not pid_file.is_file():
            continue
        try:
            pid = int(pid_file.read_text(encoding="utf-8").strip())
        except ValueError:
            raise StateToolError(f"Invalid runtime PID evidence: {pid_file}")
        if not process_exists(pid):
            continue
        raise StateToolError(f"CarsinOS is still running (live PID {pid}).")
    for relative, database in walk_files(root):
        if database.suffix.lower() in {".db", ".sqlite", ".sqlite3"}:
            assert_exclusive_database(database, relative)


def process_exists(pid: int) -> bool:
    """Use OpenProcess on Windows; os.kill(pid, 0) is not portable there."""
    if os.name == "nt":
        process = ctypes.windll.kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if process:
            ctypes.windll.kernel32.CloseHandle(process)
            return True
        error = ctypes.windll.kernel32.GetLastError()
        if error == 5:  # Access denied means live but not safely quiescent.
            raise StateToolError(f"Runtime PID cannot be proven stopped: {pid}")
        return False
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        raise StateToolError(f"Runtime PID cannot be proven stopped: {pid}")


def assert_exclusive_database(path: Path, relative: PurePosixPath) -> None:
    if os.name == "nt":
        handle = ctypes.windll.kernel32.CreateFileW(str(path), 0xC0000000, 0, None, 3, 0x80, None)
        if handle == ctypes.c_void_p(-1).value:
            raise StateToolError(f"CarsinOS state is still in use: {relative}")
        ctypes.windll.kernel32.CloseHandle(handle)
        return
    import fcntl
    try:
        with path.open("r+b") as handle:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    except OSError as error:
        raise StateToolError(f"CarsinOS state is still in use: {relative}") from error


def assert_quiescent_marker(root: Path, marker_arg: str | None) -> None:
    marker = canonical(marker_arg, root) if marker_arg else root / "launch-disabled.json"
    if not marker.is_file():
        raise StateToolError(f"Schema replacement requires an explicit launch-disabled marker: {marker}")
    data = read_json(marker)
    required = ("launch_disabled", "intake_blocked", "active_claims_fenced", "effects_reconciled")
    if not isinstance(data, dict) or any(data.get(key) is not True for key in required):
        raise StateToolError("Launch-disabled marker must attest launch_disabled, intake_blocked, active_claims_fenced, and effects_reconciled.")


def archive_outside(root: Path, archive: Path) -> None:
    try:
        archive.relative_to(root)
    except ValueError:
        return
    raise StateToolError("Archive must be outside the source/activation state root.")


def copy_included(source: Path, destination: Path) -> None:
    for relative, file in walk_files(source):
        target = destination.joinpath(*relative.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(file, target)


def atomic_rename_no_replace(source: Path, destination: Path, *, exists_message: str) -> None:
    """Use the platform's atomic no-replace rename primitive or fail closed."""
    if os.name == "nt":
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        move_file = kernel32.MoveFileExW
        move_file.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint32]
        move_file.restype = ctypes.c_int
        # MOVEFILE_WRITE_THROUGH without MOVEFILE_REPLACE_EXISTING is atomic,
        # no-replace, and does not return until the move is flushed to disk.
        if not move_file(str(source), str(destination), 0x8):
            error = ctypes.get_last_error()
            if error in {80, 183}:  # ERROR_FILE_EXISTS / ERROR_ALREADY_EXISTS
                raise StateToolError(exists_message)
            raise StateToolError(
                f"Durable rename failed: {source} -> {destination}: {ctypes.FormatError(error).strip()}"
            )
        return
    source_bytes = os.fsencode(source)
    destination_bytes = os.fsencode(destination)
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform.startswith("linux") and hasattr(libc, "renameat2"):
        rename = libc.renameat2
        rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        rename.restype = ctypes.c_int
        result = rename(-100, source_bytes, -100, destination_bytes, 1)  # AT_FDCWD, RENAME_NOREPLACE
    elif sys.platform == "darwin" and hasattr(libc, "renamex_np"):
        rename = libc.renamex_np
        rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        rename.restype = ctypes.c_int
        result = rename(source_bytes, destination_bytes, 0x4)  # RENAME_EXCL
    else:
        raise StateToolError("Atomic no-replace rename is unavailable on this platform.")
    if result != 0:
        error = ctypes.get_errno()
        if error == errno.EEXIST:
            raise StateToolError(exists_message)
        raise StateToolError(
            f"Durable rename failed: {source} -> {destination}: {os.strerror(error)}"
        )


def sync_directory(directory: Path) -> None:
    """Durably flush directory metadata on the active platform."""
    if os.name == "nt":
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        create_file = kernel32.CreateFileW
        create_file.argtypes = [
            ctypes.c_wchar_p,
            ctypes.c_uint32,
            ctypes.c_uint32,
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.c_uint32,
            ctypes.c_void_p,
        ]
        create_file.restype = ctypes.c_void_p
        flush_file_buffers = kernel32.FlushFileBuffers
        flush_file_buffers.argtypes = [ctypes.c_void_p]
        flush_file_buffers.restype = ctypes.c_int
        close_handle = kernel32.CloseHandle
        close_handle.argtypes = [ctypes.c_void_p]
        close_handle.restype = ctypes.c_int
        handle = create_file(
            str(directory),
            0x40000000,  # GENERIC_WRITE
            0x1 | 0x2 | 0x4,  # FILE_SHARE_READ | WRITE | DELETE
            None,
            3,  # OPEN_EXISTING
            0x02000000,  # FILE_FLAG_BACKUP_SEMANTICS
            None,
        )
        if handle == ctypes.c_void_p(-1).value:
            error = ctypes.get_last_error()
            raise StateToolError(f"Cannot open directory for durability sync: {directory}: {ctypes.FormatError(error).strip()}")
        try:
            if not flush_file_buffers(handle):
                error = ctypes.get_last_error()
                raise StateToolError(f"Cannot sync directory metadata: {directory}: {ctypes.FormatError(error).strip()}")
        finally:
            close_handle(handle)
        return
    flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
    descriptor = os.open(directory, flags)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def sync_tree(root: Path) -> None:
    """Flush every activated file and directory before durable finalization."""
    directories = {root}
    for current, names, files in os.walk(root, topdown=True, followlinks=False):
        current_path = Path(current)
        assert_not_reparse(current_path)
        for name in names:
            directory = current_path / name
            assert_not_reparse(directory)
            directories.add(directory)
        for name in files:
            file = current_path / name
            assert_not_reparse(file)
            with file.open("r+b") as handle:
                os.fsync(handle.fileno())
    for directory in sorted(directories, key=lambda item: len(item.parts), reverse=True):
        sync_directory(directory)


def publish_archive(staged: Path, archive: Path) -> None:
    atomic_rename_no_replace(
        staged,
        archive,
        exists_message=f"Backup archive already exists: {archive}",
    )


def validate_archive_resources(entries: list[zipfile.ZipInfo]) -> None:
    """Reject locally exhausting declared ZIP payloads before creating files."""
    if len(entries) > MAX_ARCHIVE_MEMBERS:
        raise StateToolError(
            f"Archive member count exceeds limit ({len(entries)} > {MAX_ARCHIVE_MEMBERS})."
        )
    total_uncompressed = 0
    for entry in entries:
        name = entry.filename
        if entry.file_size < 0 or entry.compress_size < 0:
            raise StateToolError(f"Archive entry has invalid declared sizes: {name}")
        if entry.is_dir() and entry.file_size != 0:
            raise StateToolError(f"Archive directory has non-zero declared size: {name}")
        if entry.file_size > MAX_ARCHIVE_ENTRY_UNCOMPRESSED_BYTES:
            raise StateToolError(
                f"Archive entry exceeds uncompressed size limit: {name}"
            )
        total_uncompressed += entry.file_size
        if total_uncompressed > MAX_ARCHIVE_TOTAL_UNCOMPRESSED_BYTES:
            raise StateToolError("Archive total uncompressed size exceeds limit.")
        if entry.file_size > 0:
            if entry.compress_size == 0:
                raise StateToolError(f"Archive entry has impossible declared compression size: {name}")
            if entry.file_size > entry.compress_size * MAX_ARCHIVE_COMPRESSION_RATIO:
                raise StateToolError(f"Archive entry exceeds declared compression ratio limit: {name}")


def make_archive(
    source: Path,
    archive: Path,
    binary_version: str,
    *,
    canaries: tuple[Canary, ...] = (),
) -> dict[str, Any]:
    binary_version = require_binary_compatibility(binary_version, label="Binary compatibility version")
    archive_outside(source, archive)
    require_clean_canaries(root=source, canaries=canaries)
    if archive.exists():
        raise StateToolError(f"Backup archive already exists: {archive}")
    archive.parent.mkdir(parents=True, exist_ok=True)
    staged_archive = archive.parent / f".{archive.name}.stage.{uuid.uuid4().hex}"
    try:
        with tempfile.TemporaryDirectory(prefix=".carsinos-backup-", dir=archive.parent) as temporary:
            stage = Path(temporary)
            failpoint("archive_source_copy.before")
            copy_included(source, stage)
            failpoint("archive_source_copy.after")
            manifest = make_manifest(stage, binary_version)
            manifest["source_state_root"] = str(source)
            write_manifest(stage, manifest)
            failpoint("archive_zip_write.before")
            with zipfile.ZipFile(staged_archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as destination:
                for relative, file in walk_files(stage, include_manifest=True):
                    destination.write(file, relative.as_posix())
            failpoint("archive_zip_write.after")
        failpoint("archive_file_sync.before")
        with staged_archive.open("rb+") as staged_file:
            staged_file.flush()
            os.fsync(staged_file.fileno())
        failpoint("archive_file_sync.after")
        verify_archive(staged_archive, canaries=canaries)
        failpoint("archive_publish_rename.before")
        publish_archive(staged_archive, archive)
        failpoint("archive_publish_rename.after")
        failpoint("archive_publish_metadata_sync.before")
        sync_directory(archive.parent)
        failpoint("archive_publish_metadata_sync.after")
    finally:
        staged_archive.unlink(missing_ok=True)
    return manifest


def extract_safe(archive: Path, destination: Path) -> None:
    with zipfile.ZipFile(archive) as source:
        entries = source.infolist()
        validate_archive_resources(entries)
        members: list[tuple[zipfile.ZipInfo, PurePosixPath]] = []
        aliases: dict[str, str] = {}
        for entry in entries:
            raw_name = entry.filename[:-1] if entry.is_dir() and entry.filename.endswith("/") else entry.filename
            relative = safe_relative(raw_name)
            name = relative.as_posix()
            alias = windows_path_alias(relative)
            if alias in aliases:
                raise StateToolError(f"Duplicate or Windows-alias archive entry: {name}")
            aliases[alias] = name
            mode = entry.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise StateToolError(f"Symlink archive entry is not allowed: {name}")
            if not is_included(relative) and name != MANIFEST_NAME:
                raise StateToolError(f"Archive contains excluded path: {name}")
            members.append((entry, relative))
        for entry, relative in members:
            if entry.is_dir():
                continue
            name = relative.as_posix()
            target = destination.joinpath(*relative.parts)
            try:
                target.relative_to(destination)
            except ValueError as error:
                raise StateToolError(f"Archive entry escapes restore directory: {name}") from error
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.open(entry, "r") as input_stream, target.open("xb") as output_stream:
                shutil.copyfileobj(input_stream, output_stream)


def verify_expanded(root: Path, *, canaries: tuple[Canary, ...] = ()) -> dict[str, Any]:
    require_clean_canaries(root=root, canaries=canaries)
    manifest_path = root / MANIFEST_NAME
    if not manifest_path.is_file():
        raise StateToolError("Backup manifest is missing.")
    manifest = read_json(manifest_path)
    if (not isinstance(manifest, dict) or manifest.get("schema") != ARCHIVE_SCHEMA or
            manifest.get("tool_schema") != TOOL_SCHEMA or manifest.get("product") != "CarsinOS"):
        raise StateToolError("Unsupported backup schema.")
    if not isinstance(manifest.get("source_state_root"), str) or not Path(manifest["source_state_root"]).is_absolute():
        raise StateToolError("Backup manifest source state root must be canonical and absolute.")
    if not valid_binary_compatibility(manifest.get("binary_compatibility_version")):
        raise StateToolError("Backup manifest binary compatibility version is missing or unknown.")
    if manifest.get("excluded_top_level") != sorted(EXCLUDED_TOP_LEVEL):
        raise StateToolError("Backup manifest excluded-top-level policy mismatch.")
    if manifest.get("secret_references") != [{"path": "secrets", "disposition": "excluded_mandatory_reauthentication"}]:
        raise StateToolError("Portable backup secret-reference disposition is invalid.")
    expected = manifest.get("files")
    if not isinstance(expected, list) or expected != sorted(expected, key=lambda item: item.get("path", "")):
        raise StateToolError("Backup manifest files must be sorted.")
    actual_paths = {relative.as_posix() for relative, _ in walk_files(root)}
    expected_paths: set[str] = set()
    expected_aliases: set[str] = set()
    for record in expected:
        if not isinstance(record, dict) or not isinstance(record.get("path"), str):
            raise StateToolError("Invalid manifest file record.")
        relative = safe_relative(record["path"])
        if not is_included(relative):
            raise StateToolError(f"Manifest contains excluded path: {relative}")
        alias = windows_path_alias(relative)
        if relative.as_posix() in expected_paths or alias in expected_aliases:
            raise StateToolError(f"Duplicate or Windows-alias manifest record: {relative}")
        expected_paths.add(relative.as_posix())
        expected_aliases.add(alias)
        path = root.joinpath(*relative.parts)
        if not path.is_file():
            raise StateToolError(f"Manifest file is missing or unsafe: {relative}")
        if path.stat().st_size != record.get("size_bytes"):
            raise StateToolError(f"Backup size mismatch: {relative}")
        if sha256(path) != record.get("sha256"):
            raise StateToolError(f"Backup checksum mismatch: {relative}")
    if actual_paths != expected_paths:
        raise StateToolError("Backup manifest is incomplete or contains untracked files.")
    if manifest.get("sqlite_databases") != sqlite_metadata(root):
        raise StateToolError("SQLite schema metadata mismatch.")
    for database in manifest.get("sqlite_databases", []):
        path = root.joinpath(*safe_relative(database["path"]).parts)
        connection = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
        try:
            integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
            foreign_keys = connection.execute("PRAGMA foreign_key_check").fetchall()
        finally:
            connection.close()
        if integrity != "ok" or foreign_keys:
            raise StateToolError(f"SQLite integrity or foreign-key check failed: {database['path']}")
    discovered_receipts = receipt_metadata(root)
    if manifest.get("receipt_metadata") != discovered_receipts:
        raise StateToolError("Receipt metadata consistency mismatch.")
    expected_generation = next((item["state_root_generation"] for item in discovered_receipts), None)
    if manifest.get("source_state_root_generation") != expected_generation:
        raise StateToolError("State-root generation metadata mismatch.")
    if manifest.get("retry_tombstones") != {"present": bool(tombstones(root)), "files": tombstones(root)}:
        raise StateToolError("Retry tombstone metadata mismatch.")
    return manifest


def verify_archive(archive: Path, *, canaries: tuple[Canary, ...] = ()) -> dict[str, Any]:
    if not archive.is_file():
        raise StateToolError(f"Backup archive not found: {archive}")
    require_clean_canaries(archive=archive, canaries=canaries)
    with tempfile.TemporaryDirectory(prefix=".carsinos-verify-", dir=archive.parent) as temporary:
        stage = Path(temporary)
        extract_safe(archive, stage)
        return verify_expanded(stage, canaries=canaries)


def resolve_receipt_verifier(repo: Path) -> Path | None:
    executable_name = "carsinos-receipt-integrity.exe" if os.name == "nt" else "carsinos-receipt-integrity"
    candidates = [repo / executable_name, repo / "bin" / executable_name]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def run_receipt_verifier(
    root: Path,
    manifest: dict[str, Any],
    action: str,
    verifier: Path | None,
) -> None:
    if not manifest.get("receipt_metadata"):
        return
    if verifier is None:
        raise StateToolError(
            "ExecAss receipt metadata requires the bundled receipt-integrity verifier before activation."
        )
    command = [str(verifier), action, "--state-root", str(root)]
    try:
        completed = subprocess.run(
            command,
            cwd=root.parent,
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise StateToolError("Receipt-integrity verifier could not be executed safely.") from error
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    try:
        result = json.loads(lines[-1]) if lines else None
    except json.JSONDecodeError as error:
        raise StateToolError("Receipt-integrity verifier returned an invalid response.") from error
    if completed.returncode != 0 or not isinstance(result, dict) or result.get("ok") is not True:
        raise StateToolError(f"Receipt-integrity verifier rejected {action}.")
    if result.get("action") != action:
        raise StateToolError("Receipt-integrity verifier returned the wrong action identity.")
    metadata = manifest.get("receipt_metadata", [])
    if len(metadata) != 1:
        raise StateToolError("Receipt-integrity activation requires exactly one authoritative receipt database.")
    expected = metadata[0]
    if action == "verify-active" and (
        result.get("status") != "trusted"
        or result.get("root_identity") != expected.get("root_identity")
        or result.get("anchor_generation") != expected.get("anchor_generation")
    ):
        raise StateToolError("Receipt-integrity verifier response does not match the activated anchor.")


def activate_stage(
    stage: Path,
    target: Path,
    force: bool,
    post_activate_check: Callable[[Path, Path | None], None],
) -> Path | None:
    parent = target.parent
    parent.mkdir(parents=True, exist_ok=True)
    rollback: Path | None = None
    removed_empty_target = False
    new_root_activated = False
    try:
        if target.exists():
            if not target.is_dir():
                raise StateToolError(f"State target is not a directory: {target}")
            if any(target.iterdir()):
                if not force:
                    raise StateToolError("State directory is not empty. Re-run with --force to preserve it as a rollback copy.")
                rollback = parent / f"{target.name}.rollback.{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
                if rollback.exists():
                    raise StateToolError(f"Rollback path already exists: {rollback}")
                failpoint("old_root_rollback_rename.before")
                atomic_rename_no_replace(
                    target,
                    rollback,
                    exists_message=f"Rollback path already exists: {rollback}",
                )
                failpoint("old_root_rollback_rename.after")
                failpoint("old_root_rollback_metadata_sync.before")
                sync_directory(parent)
                failpoint("old_root_rollback_metadata_sync.after")
            else:
                target.rmdir()
                removed_empty_target = True
                sync_directory(parent)
        failpoint("new_root_activation_rename.before")
        atomic_rename_no_replace(
            stage,
            target,
            exists_message=f"State target appeared during activation: {target}",
        )
        new_root_activated = True
        failpoint("new_root_activation_rename.after")
        failpoint("new_root_activation_metadata_sync.before")
        sync_directory(parent)
        failpoint("new_root_activation_metadata_sync.after")
        failpoint("post_activation_verification.before")
        post_activate_check(target, rollback)
        failpoint("post_activation_verification.after")
        failpoint("post_activation_finalization.before")
        sync_tree(target)
    except Exception:
        if new_root_activated and target.exists():
            if stage.exists():
                raise StateToolError("Cannot roll back activation because the stage path unexpectedly exists.")
            atomic_rename_no_replace(
                target,
                stage,
                exists_message=f"Cannot roll back activation because the stage path exists: {stage}",
            )
            sync_directory(parent)
        if rollback is not None and rollback.exists():
            atomic_rename_no_replace(
                rollback,
                target,
                exists_message=f"Cannot restore rollback because the state target exists: {target}",
            )
            sync_directory(parent)
        elif removed_empty_target and not target.exists():
            target.mkdir()
            sync_directory(parent)
        raise
    # From this point onward every activated byte and directory entry is synced,
    # verification passed, and the old root remains preserved.
    failpoint("post_activation_finalization.after")
    return rollback


def ensure_same_volume(first: Path, second: Path) -> None:
    if os.name == "nt":
        same_volume = os.path.splitdrive(str(first))[0].casefold() == os.path.splitdrive(str(second))[0].casefold()
    else:
        same_volume = first.parent.stat().st_dev == second.parent.stat().st_dev
    if not same_volume:
        raise StateToolError("Staged activation must remain on the same volume as the state target.")


def extract_to_stage(
    archive: Path,
    target: Path,
    expected_binary_version: str,
    receipt_verifier: Path | None,
    *,
    canaries: tuple[Canary, ...] = (),
) -> tuple[Path, dict[str, Any]]:
    stage = target.parent / f".{target.name}.stage.{uuid.uuid4().hex}"
    ensure_same_volume(stage, target)
    failpoint("new_root_stage_init.before")
    stage.mkdir(parents=True, exist_ok=False)
    try:
        require_clean_canaries(archive=archive, canaries=canaries)
        failpoint("new_root_stage_init.after")
        failpoint("candidate_extract_copy.before")
        extract_safe(archive, stage)
        failpoint("candidate_extract_copy.after")
        failpoint("candidate_validation.before")
        manifest = verify_expanded(stage, canaries=canaries)
        if manifest["binary_compatibility_version"] != expected_binary_version:
            raise StateToolError("Backup binary compatibility version does not match the expected active binary.")
        run_receipt_verifier(stage, manifest, "inspect-db", receipt_verifier)
        failpoint("candidate_validation.after")
        return stage, manifest
    except Exception:
        shutil.rmtree(stage, ignore_errors=True)
        raise


def verify_tombstones_preserved(old_root: Path, candidate_root: Path) -> None:
    old = tombstones(old_root)
    candidate = {item["path"]: item["sha256"] for item in tombstones(candidate_root)}
    missing = [item["path"] for item in old if candidate.get(item["path"]) != item["sha256"]]
    if missing:
        raise StateToolError("Schema replacement does not preserve retry tombstones: " + ", ".join(missing))


def post_activation_check(
    expected_binary_version: str,
    receipt_verifier: Path | None,
    preserve_tombstones: bool = False,
) -> Callable[[Path, Path | None], None]:
    def check(active_root: Path, rollback_root: Path | None) -> None:
        manifest = verify_expanded(active_root)
        if manifest["binary_compatibility_version"] != expected_binary_version:
            raise StateToolError("Activated state binary compatibility version does not match the expected active binary.")
        run_receipt_verifier(active_root, manifest, "verify-active", receipt_verifier)
        if preserve_tombstones:
            if rollback_root is None:
                raise StateToolError("Schema replacement must preserve the old root as a rollback copy.")
            verify_tombstones_preserved(rollback_root, active_root)

    return check


def main(argv: list[str] | None = None) -> int:
    parser = SafeArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("backup", "verify", "restore", "schema_replace"))
    parser.add_argument("--state-dir")
    parser.add_argument("--archive-path")
    parser.add_argument("--replacement", help="Prepared replacement state archive or root for schema_replace.")
    parser.add_argument("--binary-compatibility-version", default=os.environ.get("CARSINOS_BINARY_COMPATIBILITY_VERSION"))
    parser.add_argument("--expected-binary-compatibility-version", help="Exact binary compatibility version required before restore or schema activation.")
    parser.add_argument("--launch-disabled-marker")
    parser.add_argument("--force", action="store_true")
    arguments = parser.parse_args(argv)
    repo = Path(__file__).resolve().parent.parent
    default_state = os.environ.get("CARSINOS_STATE_DIR")
    if not default_state:
        default_state = str(Path(os.environ["LOCALAPPDATA"]) / "io.carsinos.missioncontrol" / "state") if os.environ.get("LOCALAPPDATA") else "runtime/oneclick-state"
    state_input = Path(arguments.state_dir or default_state)
    state = state_input
    try:
        state = canonical(state_input, repo)
        archive = canonical(arguments.archive_path, repo) if arguments.archive_path else None
        configure_test_failpoint()
        canaries = registered_test_canaries()
        receipt_verifier = resolve_receipt_verifier(repo)
        assert_safe_state_target(state, repo)
        if arguments.action == "backup":
            if not state.is_dir():
                raise StateToolError(f"State directory not found: {state}")
            assert_offline(state)
            archive = archive or canonical(repo / "runtime" / "backups" / f"carsinos-state-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}.zip")
            manifest = make_archive(
                state, archive, arguments.binary_compatibility_version, canaries=canaries
            )
            result = {"ok": True, "action": "backup", "root_identity": safe_path_identity(state), "archive_identity": safe_path_identity(archive), "archive_sha256": sha256(archive), "file_count": len(manifest["files"])}
        elif arguments.action == "verify":
            if archive is None:
                raise StateToolError("--archive-path is required for verify.")
            manifest = verify_archive(archive, canaries=canaries)
            result = {"ok": True, "action": "verify", "root_identity": safe_path_identity(state), "archive_identity": safe_path_identity(archive), "file_count": len(manifest["files"])}
        elif arguments.action == "restore":
            if archive is None:
                raise StateToolError("--archive-path is required for restore.")
            expected_binary = require_binary_compatibility(arguments.expected_binary_compatibility_version, label="Expected binary compatibility version")
            assert_offline(state) if state.exists() else None
            archive_outside(state, archive)
            stage, manifest = extract_to_stage(
                archive, state, expected_binary, receipt_verifier, canaries=canaries
            )
            rollback = activate_stage(
                stage,
                state,
                arguments.force,
                post_activation_check(expected_binary, receipt_verifier),
            )
            result = {"ok": True, "action": "restore", "root_identity": safe_path_identity(state), "rollback_preserved": rollback is not None, "file_count": len(manifest["files"]), "reauthentication_required": True}
        else:
            if archive is None or not arguments.replacement:
                raise StateToolError("schema_replace requires --archive-path for the old-root archive and --replacement.")
            expected_binary = require_binary_compatibility(arguments.expected_binary_compatibility_version, label="Expected binary compatibility version")
            if not state.is_dir():
                raise StateToolError(f"State directory not found: {state}")
            assert_offline(state)
            assert_quiescent_marker(state, arguments.launch_disabled_marker)
            archive_outside(state, archive)
            make_archive(
                state, archive, arguments.binary_compatibility_version, canaries=canaries
            )
            replacement = canonical(arguments.replacement, repo)
            if replacement.is_file():
                archive_outside(state, replacement)
                stage, manifest = extract_to_stage(
                    replacement, state, expected_binary, receipt_verifier, canaries=canaries
                )
            elif replacement.is_dir():
                if paths_overlap(replacement, state) or paths_overlap(replacement, archive) or replacement == state.parent:
                    raise StateToolError("Prepared replacement root must not overlap the state root, archive, or activation staging parent.")
                if replacement.name.startswith(f".{state.name}.stage.") or replacement.name.startswith(f".{state.name}.replacement."):
                    raise StateToolError("Prepared replacement root cannot be an activation staging path.")
                assert_offline(replacement)
                prepared = state.parent / f".{state.name}.replacement.{uuid.uuid4().hex}.zip"
                # The old-root archive records the currently active binary contract;
                # a prepared replacement root must be stamped for the binary that
                # will activate it. Those versions may intentionally differ during
                # an incompatible schema replacement.
                make_archive(replacement, prepared, expected_binary, canaries=canaries)
                try:
                    stage, manifest = extract_to_stage(
                        prepared, state, expected_binary, receipt_verifier, canaries=canaries
                    )
                finally:
                    prepared.unlink(missing_ok=True)
            else:
                raise StateToolError(f"Prepared replacement root/archive not found: {replacement}")
            try:
                verify_tombstones_preserved(state, stage)
                failpoint("pre_schema_activation")
                rollback = activate_stage(
                    stage,
                    state,
                    True,
                    post_activation_check(
                        expected_binary,
                        receipt_verifier,
                        preserve_tombstones=True,
                    ),
                )
            except Exception:
                shutil.rmtree(stage, ignore_errors=True)
                raise
            result = {"ok": True, "action": "schema_replace", "root_identity": safe_path_identity(state), "archive_identity": safe_path_identity(archive), "rollback_preserved": rollback is not None, "file_count": len(manifest["files"])}
        print(json.dumps(result, sort_keys=True))
        print(f"CarsinOS state {result['action']} complete.")
        return 0
    except Exception as error:
        reason = safe_error_category(error)
        print(json.dumps({"ok": False, "action": arguments.action, "reason": reason, "root_identity": safe_path_identity(state)}, sort_keys=True), file=sys.stderr)
        print(f"CarsinOS state {arguments.action} failed ({reason}).", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
