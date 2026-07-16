#!/usr/bin/env python3
"""Create a deterministic ZIP archive from explicit files or directories."""

from __future__ import annotations

import argparse
import datetime as dt
import os
from pathlib import Path
import stat
import zipfile


def archive_timestamp() -> tuple[int, int, int, int, int, int]:
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "1767225600"))
    value = dt.datetime.fromtimestamp(epoch, tz=dt.timezone.utc)
    if value.year < 1980:
        value = value.replace(year=1980, month=1, day=1, hour=0, minute=0, second=0)
    return value.year, value.month, value.day, value.hour, value.minute, value.second


def collect(root: Path, entries: list[str]) -> list[Path]:
    files: set[Path] = set()
    for raw in entries:
        entry = (root / raw).resolve()
        try:
            entry.relative_to(root.resolve())
        except ValueError as error:
            raise SystemExit(f"Entry escapes the source root: {raw}") from error
        if entry.is_file():
            files.add(entry)
        elif entry.is_dir():
            files.update(path for path in entry.rglob("*") if path.is_file())
        else:
            raise SystemExit(f"Archive entry does not exist: {raw}")
    return sorted(files, key=lambda path: path.relative_to(root).as_posix())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("entries", nargs="+")
    arguments = parser.parse_args()

    root = Path(arguments.root).resolve()
    output = Path(arguments.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    timestamp = archive_timestamp()

    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for source in collect(root, arguments.entries):
            relative = source.relative_to(root).as_posix()
            info = zipfile.ZipInfo(relative, date_time=timestamp)
            info.create_system = 3
            mode = source.stat().st_mode
            permissions = 0o755 if mode & stat.S_IXUSR else 0o644
            info.external_attr = (permissions & 0xFFFF) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, source.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


if __name__ == "__main__":
    main()
