#!/usr/bin/env python3
"""Create one adversarial ZIP mutation for artifact-package policy tests."""

from __future__ import annotations

import argparse
from pathlib import Path
import stat
import zipfile


PACKAGE_ROOT = "nara-reference-game"
README_ENTRY = f"{PACKAGE_ROOT}/README.md"
MISSING_ENTRY = f"{PACKAGE_ROOT}/project/scenes/startup.scene.json"


def regular_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def copied_info(original: zipfile.ZipInfo, mode: str) -> zipfile.ZipInfo:
    name = (
        f"{PACKAGE_ROOT}/./README.md"
        if mode == "path-alias" and original.filename == README_ENTRY
        else original.filename
    )
    info = regular_info(name)
    if mode == "special-mode" and original.filename == README_ENTRY:
        info.external_attr = (stat.S_IFLNK | 0o777) << 16
    elif original.external_attr:
        info.external_attr = original.external_attr
    return info


def mutate(source: Path, output: Path, mode: str) -> None:
    if output.exists():
        raise ValueError("output already exists")
    with zipfile.ZipFile(source, "r") as input_archive, zipfile.ZipFile(
        output,
        "x",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as output_archive:
        for original in input_archive.infolist():
            if mode == "missing-entry" and original.filename == MISSING_ENTRY:
                continue
            contents = input_archive.read(original)
            if mode == "digest-mismatch" and original.filename == README_ENTRY:
                if not contents:
                    raise ValueError("README fixture must not be empty")
                contents = bytes((contents[0] ^ 1,)) + contents[1:]
            output_archive.writestr(copied_info(original, mode), contents)
        if mode == "unexpected-entry":
            output_archive.writestr(
                regular_info(f"{PACKAGE_ROOT}/unexpected.txt"), b"unexpected"
            )
        elif mode == "traversal":
            output_archive.writestr(regular_info("../escape.txt"), b"escape")
        elif mode == "case-collision":
            output_archive.writestr(
                regular_info(f"{PACKAGE_ROOT}/readme.md"), b"collision"
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "mode",
        choices=(
            "unexpected-entry",
            "traversal",
            "case-collision",
            "digest-mismatch",
            "special-mode",
            "missing-entry",
            "path-alias",
        ),
    )
    parser.add_argument("source")
    parser.add_argument("output")
    arguments = parser.parse_args()
    mutate(Path(arguments.source), Path(arguments.output), arguments.mode)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
