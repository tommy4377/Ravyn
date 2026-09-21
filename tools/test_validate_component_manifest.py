#!/usr/bin/env python3
"""Regression tests for the component catalogue validator."""

from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from validate_component_manifest import ValidationError, validate_manifest, verify_archive_member


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def artifact(**overrides: object) -> dict[str, object]:
    value: dict[str, object] = {
        "engine": "tool",
        "version": "1.0.0",
        "target": "x86_64-pc-windows-msvc",
        "url": "https://example.invalid/tool.exe",
        "sha256": "0" * 64,
        "size_bytes": 2,
        "filename": "tool.exe",
        "capabilities": ["probe"],
    }
    value.update(overrides)
    return value


def manifest(*artifacts: dict[str, object]) -> dict[str, object]:
    return {"schema_version": 1, "channel": "stable", "artifacts": list(artifacts)}


class ManifestValidationTests(unittest.TestCase):
    def test_accepts_valid_direct_artifact(self) -> None:
        values = validate_manifest(manifest(artifact()))
        self.assertEqual(len(values), 1)


    def test_accepts_bounded_unknown_size_and_nested_installer_output(self) -> None:
        values = validate_manifest(
            manifest(
                artifact(
                    url="https://example.invalid/tool.msi",
                    size_bytes=0,
                    max_size_bytes=4096,
                    filename="Files/Tool/tool.exe",
                    installer={"kind": "msi_administrative"},
                )
            )
        )
        self.assertEqual(values[0]["max_size_bytes"], 4096)

    def test_rejects_unknown_size_without_a_bound(self) -> None:
        with self.assertRaisesRegex(ValidationError, "max_size_bytes is required"):
            validate_manifest(manifest(artifact(size_bytes=0)))

    def test_rejects_installer_and_archive_strategy_combination(self) -> None:
        with self.assertRaisesRegex(ValidationError, "mutually exclusive"):
            validate_manifest(
                manifest(
                    artifact(
                        url="https://example.invalid/tool.msi",
                        archive_member="bin/tool.exe",
                        member_sha256="1" * 64,
                        installer={"kind": "msi_administrative"},
                    )
                )
            )

    def test_rejects_duplicate_engine_target_pair(self) -> None:
        with self.assertRaisesRegex(ValidationError, "duplicate artifact"):
            validate_manifest(manifest(artifact(), artifact(version="2.0.0")))

    def test_rejects_unsafe_archive_member(self) -> None:
        with self.assertRaisesRegex(ValidationError, "unsafe path segment"):
            validate_manifest(
                manifest(
                    artifact(
                        url="https://example.invalid/tool.zip",
                        archive_member="../tool.exe",
                        member_sha256="1" * 64,
                    )
                )
            )

    def test_rejects_non_https_url(self) -> None:
        with self.assertRaisesRegex(ValidationError, "must use HTTPS"):
            validate_manifest(manifest(artifact(url="http://example.invalid/tool.exe")))

    def test_verifies_selected_zip_member(self) -> None:
        payload = b"MZtest-binary"
        with tempfile.TemporaryDirectory() as temp:
            archive_path = Path(temp) / "tool.zip"
            with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("bin/tool.exe", payload)
            verify_archive_member(archive_path, "bin/tool.exe", sha256(payload))

    def test_rejects_wrong_zip_member_hash(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            archive_path = Path(temp) / "tool.zip"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("bin/tool.exe", b"MZtest-binary")
            with self.assertRaisesRegex(ValidationError, "SHA-256 mismatch"):
                verify_archive_member(archive_path, "bin/tool.exe", "0" * 64)


if __name__ == "__main__":
    unittest.main()
