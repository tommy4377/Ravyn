#!/usr/bin/env python3
"""Validate Ravyn component catalogues and optionally verify remote artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
TOKEN_RE = re.compile(r"^[A-Za-z0-9._+-]+$")


class ValidationError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"cannot read {path}: {error}") from error
    require(isinstance(value, dict), "manifest root must be an object")
    return value


def validate_relative_path(value: str, label: str) -> None:
    path = PurePosixPath(value)
    require(value == value.strip() and value != "", f"{label} must be non-empty")
    require("\\" not in value, f"{label} must use forward slashes")
    require(not path.is_absolute(), f"{label} must be relative")
    require(all(part not in {"", ".", ".."} and ":" not in part for part in path.parts), f"{label} contains an unsafe path segment")


def validate_member_path(value: str) -> None:
    validate_relative_path(value, "archive_member")


def validate_artifact(artifact: Any, index: int) -> tuple[str, str]:
    prefix = f"artifacts[{index}]"
    require(isinstance(artifact, dict), f"{prefix} must be an object")
    for field in ("engine", "version", "target", "url", "sha256", "size_bytes", "filename"):
        require(field in artifact, f"{prefix}.{field} is required")

    engine = artifact["engine"]
    target = artifact["target"]
    require(isinstance(engine, str) and TOKEN_RE.fullmatch(engine) is not None, f"{prefix}.engine is invalid")
    require(isinstance(artifact["version"], str) and TOKEN_RE.fullmatch(artifact["version"]) is not None, f"{prefix}.version is invalid")
    require(isinstance(target, str) and TOKEN_RE.fullmatch(target) is not None, f"{prefix}.target is invalid")

    filename = artifact["filename"]
    require(isinstance(filename, str), f"{prefix}.filename must be a string")
    validate_relative_path(filename, f"{prefix}.filename")

    parsed = urllib.parse.urlsplit(artifact["url"])
    require(parsed.scheme == "https", f"{prefix}.url must use HTTPS")
    require(parsed.hostname is not None, f"{prefix}.url must include a host")
    require(parsed.username is None and parsed.password is None, f"{prefix}.url must not contain credentials")
    require(parsed.fragment == "", f"{prefix}.url must not contain a fragment")

    require(isinstance(artifact["sha256"], str) and SHA256_RE.fullmatch(artifact["sha256"]) is not None, f"{prefix}.sha256 is invalid")
    size = artifact["size_bytes"]
    maximum = artifact.get("max_size_bytes")
    require(isinstance(size, int) and 0 <= size <= MAX_ARTIFACT_BYTES, f"{prefix}.size_bytes is outside the allowed range")
    if size == 0:
        require(isinstance(maximum, int) and 0 < maximum <= MAX_ARTIFACT_BYTES, f"{prefix}.max_size_bytes is required when size_bytes is zero")
    else:
        require(maximum is None, f"{prefix}.max_size_bytes is only valid when size_bytes is zero")

    capabilities = artifact.get("capabilities", [])
    require(isinstance(capabilities, list) and all(isinstance(value, str) and value for value in capabilities), f"{prefix}.capabilities must be a string array")
    require(len(capabilities) == len(set(capabilities)), f"{prefix}.capabilities contains duplicates")

    member = artifact.get("archive_member")
    member_sha = artifact.get("member_sha256")
    installer = artifact.get("installer")
    if member is None:
        require(member_sha is None, f"{prefix}.member_sha256 requires archive_member")
    else:
        require(isinstance(member, str), f"{prefix}.archive_member must be a string")
        validate_member_path(member)
        require(isinstance(member_sha, str) and SHA256_RE.fullmatch(member_sha) is not None, f"{prefix}.member_sha256 is invalid")

    if installer is not None:
        require(member is None, f"{prefix}.installer and archive_member are mutually exclusive")
        require(isinstance(installer, dict), f"{prefix}.installer must be an object")
        require(installer.get("kind") == "msi_administrative", f"{prefix}.installer.kind is unsupported")
        require(parsed.path.lower().endswith(".msi"), f"{prefix}.msi_administrative requires an MSI URL")
        require("windows" in target.lower(), f"{prefix}.msi_administrative requires a Windows target")

    return engine, target


def validate_manifest(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    require(manifest.get("schema_version") == 1, "schema_version must be 1")
    channel = manifest.get("channel")
    require(isinstance(channel, str) and TOKEN_RE.fullmatch(channel) is not None, "channel is invalid")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, list), "artifacts must be an array")
    require(len(artifacts) <= 256, "manifest contains more than 256 artifacts")

    identities: set[tuple[str, str]] = set()
    for index, artifact in enumerate(artifacts):
        identity = validate_artifact(artifact, index)
        require(identity not in identities, f"duplicate artifact for engine={identity[0]} target={identity[1]}")
        identities.add(identity)
    return artifacts


def stream_download(url: str, destination: Path, exact_size: int | None, size_limit: int) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": "Ravyn-Component-Validator/1"})
    digest = hashlib.sha256()
    downloaded = 0
    try:
        with urllib.request.urlopen(request, timeout=120) as response, destination.open("wb") as output:
            final_url = urllib.parse.urlsplit(response.geturl())
            require(final_url.scheme == "https", "artifact redirect left HTTPS")
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                downloaded += len(chunk)
                require(downloaded <= size_limit, "artifact exceeded its signed size limit")
                output.write(chunk)
                digest.update(chunk)
    except (OSError, urllib.error.URLError) as error:
        raise ValidationError(f"download failed for {url}: {error}") from error
    if exact_size is not None:
        require(downloaded == exact_size, f"artifact size mismatch: expected {exact_size}, downloaded {downloaded}")
    return digest.hexdigest()

def verify_archive_member(path: Path, member: str, expected_sha256: str) -> None:
    try:
        with zipfile.ZipFile(path) as archive:
            info = archive.getinfo(member)
            require(not info.is_dir(), f"archive member {member!r} is a directory")
            require(info.file_size <= MAX_ARTIFACT_BYTES, f"archive member {member!r} is too large")
            digest = hashlib.sha256()
            read = 0
            with archive.open(info) as stream:
                while True:
                    chunk = stream.read(1024 * 1024)
                    if not chunk:
                        break
                    read += len(chunk)
                    require(read <= MAX_ARTIFACT_BYTES, f"archive member {member!r} exceeded the extraction limit")
                    digest.update(chunk)
    except (KeyError, OSError, zipfile.BadZipFile) as error:
        raise ValidationError(f"cannot verify archive member {member!r}: {error}") from error
    require(digest.hexdigest().lower() == expected_sha256.lower(), f"archive member {member!r} SHA-256 mismatch")


def verify_installer_output(artifact: dict[str, Any], package: Path, temp_root: Path) -> None:
    installer = artifact.get("installer")
    if installer is None:
        return
    require(os.name == "nt", "installer provisioning verification requires Windows")
    target = temp_root / f"{artifact['engine']}-{artifact['target']}-installed"
    target.mkdir()
    try:
        completed = subprocess.run(
            ["msiexec.exe", "/a", str(package), "/qn", "/norestart", f"TARGETDIR={target}"],
            check=False,
            timeout=180,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValidationError(f"cannot provision {artifact['engine']} installer: {error}") from error
    require(completed.returncode == 0, f"installer provisioning exited with {completed.returncode}")
    executable = target.joinpath(*PurePosixPath(artifact["filename"]).parts)
    require(executable.is_file(), f"installer did not produce {artifact['filename']}")
    with executable.open("rb") as stream:
        require(stream.read(2) == b"MZ", f"installed executable {artifact['filename']} does not have an MZ header")


def verify_artifact(artifact: dict[str, Any], temp_root: Path, provision: bool = False) -> None:
    identity = f"{artifact['engine']} {artifact['version']} {artifact['target']}"
    destination = temp_root / f"{artifact['engine']}-{artifact['target']}.artifact"
    exact_size = artifact["size_bytes"] or None
    size_limit = exact_size or artifact["max_size_bytes"]
    actual_sha = stream_download(artifact["url"], destination, exact_size, size_limit)
    require(actual_sha.lower() == artifact["sha256"].lower(), f"{identity}: artifact SHA-256 mismatch")
    member = artifact.get("archive_member")
    if member is not None:
        verify_archive_member(destination, member, artifact["member_sha256"])
    elif artifact.get("installer") is not None:
        if provision:
            verify_installer_output(artifact, destination, temp_root)
    elif artifact["filename"].lower().endswith(".exe"):
        with destination.open("rb") as stream:
            require(stream.read(2) == b"MZ", f"{identity}: Windows executable does not have an MZ header")
    print(f"verified {identity}")

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", nargs="?", default="assets/engines/stable.json", type=Path)
    parser.add_argument("--download", action="store_true", help="download and verify every selected artifact")
    parser.add_argument("--provision", action="store_true", help="also execute fixed installer extraction strategies; implies --download")
    parser.add_argument("--target", action="append", default=[], help="verify only this target triple; may be repeated")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        manifest = load_manifest(args.manifest)
        artifacts = validate_manifest(manifest)
        selected = [artifact for artifact in artifacts if not args.target or artifact["target"] in args.target]
        require(selected or not artifacts, "target filter did not select any artifacts")
        print(f"validated {len(artifacts)} manifest artifacts ({len(selected)} selected)")
        if args.download or args.provision:
            with tempfile.TemporaryDirectory(prefix="ravyn-component-validation-") as temp:
                temp_root = Path(temp)
                for artifact in selected:
                    verify_artifact(artifact, temp_root, provision=args.provision)
        return 0
    except ValidationError as error:
        print(f"component manifest validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
