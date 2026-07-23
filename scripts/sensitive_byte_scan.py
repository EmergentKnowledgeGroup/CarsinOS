"""Fail-closed raw-byte canary scanning for CarsinOS state-tool proof.

The scanner intentionally accepts only runtime-provided canaries.  It never
serializes values, and every finding names a variant and surface without
including the sensitive bytes themselves.
"""
from __future__ import annotations

import base64
import hashlib
import os
import re
import stat
import urllib.parse
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


MAX_SCAN_FILES = 100_000
MAX_SCAN_FILE_BYTES = 16 * 1024 * 1024 * 1024
MAX_SCAN_TOTAL_BYTES = 128 * 1024 * 1024 * 1024
SCAN_CHUNK_BYTES = 1024 * 1024
PROTECTED_SECRET_TOP_LEVEL = frozenset({"secrets"})


class SensitiveByteScanError(RuntimeError):
    pass


@dataclass(frozen=True)
class Canary:
    identifier: str
    value: str

    def variants(self) -> dict[str, bytes]:
        raw = self.value.encode("utf-8")
        utf16le = self.value.encode("utf-16le")
        utf16be = self.value.encode("utf-16be")
        percent_upper = urllib.parse.quote_from_bytes(raw, safe="")
        percent_lower = re.sub(r"%[0-9A-F]{2}", lambda match: match.group(0).lower(), percent_upper)
        encoded: dict[str, bytes] = {
            "utf8": raw,
            "utf16le": utf16le,
            "utf16le_bom": b"\xff\xfe" + utf16le,
            "utf16be": utf16be,
            "utf16be_bom": b"\xfe\xff" + utf16be,
            "base64_padded": base64.b64encode(raw),
            "base64_unpadded": base64.b64encode(raw).rstrip(b"="),
            "base64url_padded": base64.urlsafe_b64encode(raw),
            "base64url_unpadded": base64.urlsafe_b64encode(raw).rstrip(b"="),
            "hex_lower": raw.hex().encode("ascii"),
            "hex_upper": raw.hex().upper().encode("ascii"),
            "percent_upper": percent_upper.encode("ascii"),
            "percent_lower": percent_lower.encode("ascii"),
            "plus_query": urllib.parse.quote_plus(self.value, safe="").encode("ascii"),
        }
        encoded["double_percent"] = urllib.parse.quote_from_bytes(
            encoded["percent_upper"], safe=""
        ).encode("ascii")
        for name, variant in (
            ("utf16le", utf16le),
            ("utf16le_bom", b"\xff\xfe" + utf16le),
            ("utf16be", utf16be),
            ("utf16be_bom", b"\xfe\xff" + utf16be),
        ):
            encoded[f"base64_{name}"] = base64.b64encode(variant)
            encoded[f"hex_{name}"] = variant.hex().encode("ascii")
        return encoded


@dataclass(frozen=True, order=True)
class SensitiveFinding:
    label: str
    surface: str
    locator: str


def make_runtime_canary(identifier: str = "ea111") -> Canary:
    # Delimiters force meaningful query/percent encodings while the random part
    # supplies high entropy.  The value is never emitted by this module.
    return Canary(identifier=identifier, value=f"{base64.urlsafe_b64encode(os.urandom(32)).decode('ascii')} /?+")


def _patterns(canaries: Iterable[Canary]) -> dict[bytes, list[str]]:
    patterns: dict[bytes, list[str]] = {}
    for canary in canaries:
        if not canary.identifier or not canary.value:
            raise SensitiveByteScanError("Canary registration is invalid.")
        for name, value in canary.variants().items():
            if not value:
                raise SensitiveByteScanError("Canary variant is invalid.")
            patterns.setdefault(value, []).append(f"{canary.identifier}:{name}")
    return patterns


def _find_bytes(data: bytes, patterns: dict[bytes, list[str]], surface: str, locator: str) -> list[SensitiveFinding]:
    found: list[SensitiveFinding] = []
    safe_locator = "sha256:" + hashlib.sha256(
        locator.encode("utf-8", "surrogateescape")
    ).hexdigest()[:24]
    for pattern, labels in patterns.items():
        if pattern in data:
            found.extend(SensitiveFinding(label, surface, safe_locator) for label in labels)
    return found


def _state_surface(relative: str) -> str:
    lowered = relative.casefold()
    first = lowered.split("/", 1)[0]
    if lowered.endswith((".db", ".db-wal", ".db-shm", ".sqlite", ".sqlite3")):
        return "state_database"
    if first in {"logs", "outbox", "notifications", "exports", "urls"}:
        return f"state_{first}"
    if lowered == "backup-manifest.json":
        return "state_manifest"
    return "state_file"


def _scan_reader(source: object, patterns: dict[bytes, list[str]], surface: str, locator: str, *, maximum: int) -> list[SensitiveFinding]:
    read = getattr(source, "read", None)
    if not callable(read):
        raise SensitiveByteScanError("Scan target does not provide a readable byte stream.")
    try:
        remaining = maximum
        overlap = max((len(pattern) for pattern in patterns), default=1) - 1
        tail = b""
        findings: list[SensitiveFinding] = []
        while True:
            chunk = read(SCAN_CHUNK_BYTES)
            if not chunk:
                return findings
            remaining -= len(chunk)
            if remaining < 0:
                raise SensitiveByteScanError("Scan target exceeds the declared byte limit.")
            findings.extend(_find_bytes(tail + chunk, patterns, surface, locator))
            tail = (tail + chunk)[-overlap:] if overlap else b""
    except (OSError, RuntimeError, zipfile.BadZipFile) as error:
        raise SensitiveByteScanError("Cannot read a scan target.") from error


def _scan_stream(path: Path, patterns: dict[bytes, list[str]], surface: str, locator: str) -> list[SensitiveFinding]:
    try:
        size = path.stat().st_size
    except OSError as error:
        raise SensitiveByteScanError("Cannot inspect a scan target.") from error
    if size > MAX_SCAN_FILE_BYTES:
        raise SensitiveByteScanError("Scan target exceeds the per-file byte limit.")
    try:
        with path.open("rb") as source:
            return _scan_reader(source, patterns, surface, locator, maximum=size)
    except OSError as error:
        raise SensitiveByteScanError("Cannot read a scan target.") from error


def scan_tree(root: Path, canaries: Iterable[Canary], *, surface: str = "state") -> list[SensitiveFinding]:
    patterns = _patterns(canaries)
    if not patterns:
        return []
    if not root.is_dir():
        raise SensitiveByteScanError("Scan root is not a directory.")
    findings: list[SensitiveFinding] = []
    scanned_bytes = 0
    scanned_files = 0
    for current, directories, files in os.walk(root, topdown=True, followlinks=False):
        current_path = Path(current)
        try:
            current_info = current_path.lstat()
        except OSError as error:
            raise SensitiveByteScanError("Cannot inspect a scan directory.") from error
        if stat.S_ISLNK(current_info.st_mode) or getattr(current_info, "st_file_attributes", 0) & 0x400:
            raise SensitiveByteScanError("Reparse point is not allowed in a scan root.")
        relative_directory = current_path.relative_to(root)
        if relative_directory.parts and relative_directory.parts[0] in PROTECTED_SECRET_TOP_LEVEL:
            directories[:] = []
            continue
        safe_directories: list[str] = []
        for name in directories:
            candidate = current_path / name
            try:
                info = candidate.lstat()
            except OSError as error:
                raise SensitiveByteScanError("Cannot inspect a scan directory.") from error
            if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
                raise SensitiveByteScanError("Reparse point is not allowed in a scan root.")
            if not relative_directory.parts and name in PROTECTED_SECRET_TOP_LEVEL:
                continue
            safe_directories.append(name)
        directories[:] = safe_directories
        for name in files:
            candidate = current_path / name
            try:
                info = candidate.lstat()
            except OSError as error:
                raise SensitiveByteScanError("Cannot inspect a scan file.") from error
            if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode):
                raise SensitiveByteScanError("Non-regular file is not allowed in a scan root.")
            scanned_files += 1
            scanned_bytes += info.st_size
            if scanned_files > MAX_SCAN_FILES or scanned_bytes > MAX_SCAN_TOTAL_BYTES:
                raise SensitiveByteScanError("Scan resource limit exceeded.")
            relative = candidate.relative_to(root).as_posix()
            findings.extend(_find_bytes(relative.encode("utf-8", "surrogateescape"), patterns, f"{surface}_path", f"path:{relative}"))
            findings.extend(_scan_stream(candidate, patterns, _state_surface(relative), f"file:{relative}"))
    return sorted(set(findings))


def scan_zip(archive: Path, canaries: Iterable[Canary]) -> list[SensitiveFinding]:
    patterns = _patterns(canaries)
    if not patterns:
        return []
    raw_findings = _scan_stream(archive, patterns, "archive_raw", "zip_bytes")
    try:
        with zipfile.ZipFile(archive) as source:
            entries = source.infolist()
            if len(entries) > MAX_SCAN_FILES:
                raise SensitiveByteScanError("Scan archive member count exceeds the byte limit.")
            total = 0
            findings = list(raw_findings)
            for entry in entries:
                total += entry.file_size
                if entry.file_size > MAX_SCAN_FILE_BYTES or total > MAX_SCAN_TOTAL_BYTES:
                    raise SensitiveByteScanError("Scan archive resource limit exceeded.")
                findings.extend(_find_bytes(entry.filename.encode("utf-8", "surrogateescape"), patterns, "archive_member_name", entry.filename))
                if entry.is_dir():
                    continue
                try:
                    with source.open(entry, "r") as member:
                        findings.extend(
                            _scan_reader(member, patterns, "archive_member", entry.filename, maximum=entry.file_size)
                        )
                except (OSError, RuntimeError, zipfile.BadZipFile) as error:
                    raise SensitiveByteScanError("Cannot read an archive member for scanning.") from error
    except (OSError, zipfile.BadZipFile) as error:
        raise SensitiveByteScanError("Cannot read archive for scanning.") from error
    return sorted(set(findings))


def require_clean(findings: Iterable[SensitiveFinding]) -> None:
    ordered = sorted(set(findings))
    if ordered:
        summary = ", ".join(f"{item.label}@{item.surface}:{item.locator}" for item in ordered)
        raise SensitiveByteScanError(f"Sensitive-byte canary detected ({summary}).")


def require_detected_labels(expected: Iterable[str], findings: Iterable[SensitiveFinding]) -> None:
    observed = {finding.label for finding in findings}
    missing = sorted(set(expected) - observed)
    if missing:
        raise SensitiveByteScanError("Sensitive-byte scanner missed declared canary variants: " + ", ".join(missing))
