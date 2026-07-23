#!/usr/bin/env python3
"""Create one bounded, checkout-free reference-game candidate ZIP archive."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import sys
import tempfile
from typing import Any, BinaryIO, Iterable, Sequence
import zipfile


PACKAGE_MANIFEST_SCHEMA = "nara.reference-game.candidate-package-v1"
BUNDLE_MANIFEST_SCHEMA = "nara.reference-game.candidate-transport-bundle-v1"
LAYOUT_SCHEMA = "nara.reference-game.package-layout-v1"
FORMAT_VERSION = 1
ARCHIVE_SUFFIX = ".zip"
MANIFEST_NAME = "manifest.json"
CHUNK_SIZE = 64 * 1024
MAX_BUNDLE_HELPER_BYTES = 512 * 1024
MAX_BUNDLE_RECEIPT_BYTES = 64 * 1024
MAX_JSON_BYTES = 64 * 1024
MAX_PATH_BYTES = 512
ZIP_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
PLATFORM_SUFFIXES = {
    "windows-x86_64": ".exe",
    "linux-x86_64": "",
}
VERSION_PATTERN = re.compile(
    r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
REVISION_PATTERN = re.compile(r"^[0-9a-f]{40}$")


class PackageError(Exception):
    """A candidate package violates its fixed local contract."""


@dataclass(frozen=True)
class PackageLimits:
    max_file_count: int
    max_file_bytes: int
    max_expanded_bytes: int
    max_encoded_bytes: int
    max_manifest_bytes: int


@dataclass(frozen=True)
class Layout:
    package_root: str
    documentation: tuple[tuple[str, str], ...]
    project_files: tuple[str, ...]
    limits: PackageLimits


@dataclass(frozen=True)
class StagedFile:
    source: Path
    destination: str
    executable: bool


@dataclass(frozen=True)
class FileDescriptor:
    path: str
    size_bytes: int
    sha256: str
    executable: bool


def tool_root() -> Path:
    return Path(__file__).resolve().parent


def layout_path() -> Path:
    return tool_root().parent / "packaging" / "package-layout-v1.json"


def path_is_within(candidate: Path, parent: Path) -> bool:
    try:
        candidate.relative_to(parent)
    except ValueError:
        return False
    return True


def path_has_link_or_reparse_point(path: Path) -> bool:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise PackageError("candidate path metadata could not be read") from error
    file_attributes = getattr(metadata, "st_file_attributes", 0)
    reparse_point = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return stat.S_ISLNK(metadata.st_mode) or bool(file_attributes & reparse_point)


def require_regular_file(path: Path, subject: str) -> None:
    if path_has_link_or_reparse_point(path):
        raise PackageError(f"{subject} must be a regular non-link file")
    try:
        metadata = path.stat()
    except OSError as error:
        raise PackageError(f"{subject} could not be read") from error
    if not stat.S_ISREG(metadata.st_mode):
        raise PackageError(f"{subject} must be a regular non-link file")


def require_directory(path: Path, subject: str) -> None:
    if path_has_link_or_reparse_point(path):
        raise PackageError(f"{subject} must be a non-link directory")
    try:
        metadata = path.stat()
    except OSError as error:
        raise PackageError(f"{subject} could not be read") from error
    if not stat.S_ISDIR(metadata.st_mode):
        raise PackageError(f"{subject} must be a non-link directory")


def read_file_bounded(path: Path, subject: str, maximum_bytes: int) -> bytes:
    require_regular_file(path, subject)
    try:
        with path.open("rb") as source:
            contents = source.read(maximum_bytes + 1)
    except OSError as error:
        raise PackageError(f"{subject} could not be read") from error
    if len(contents) > maximum_bytes:
        raise PackageError(f"{subject} exceeds its byte limit")
    return contents


def parse_relative_path(value: object, subject: str) -> str:
    if not isinstance(value, str) or not value:
        raise PackageError(f"{subject} must be a non-empty path")
    if (
        not value.isascii()
        or len(value) > MAX_PATH_BYTES
        or "\\" in value
        or ":" in value
        or "\x00" in value
    ):
        raise PackageError(f"{subject} must be an ASCII slash-separated relative path")
    path = PurePosixPath(value)
    if (
        path.is_absolute()
        or path.as_posix() != value
        or any(part in {"", ".", ".."} for part in path.parts)
        or any(len(part) > 255 for part in path.parts)
    ):
        raise PackageError(f"{subject} must be a normalized relative path")
    return path.as_posix()


def parse_positive_limit(value: object, subject: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise PackageError(f"{subject} must be a positive integer")
    return value


def load_layout() -> Layout:
    source = layout_path()
    raw = read_file_bounded(source, "package layout", MAX_JSON_BYTES)
    try:
        decoded = json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as error:
        raise PackageError("package layout is not valid JSON") from error
    if not isinstance(decoded, dict) or set(decoded) != {
        "schema",
        "package_root",
        "documentation",
        "project_files",
        "limits",
    }:
        raise PackageError("package layout has an unexpected shape")
    if decoded["schema"] != LAYOUT_SCHEMA:
        raise PackageError("package layout schema is unsupported")
    package_root = parse_relative_path(decoded["package_root"], "package root")
    if "/" in package_root:
        raise PackageError("package root must be one path segment")
    documentation_value = decoded["documentation"]
    if not isinstance(documentation_value, list) or not documentation_value:
        raise PackageError("package documentation must be a non-empty array")
    documentation: list[tuple[str, str]] = []
    for index, mapping in enumerate(documentation_value):
        if not isinstance(mapping, dict) or set(mapping) != {"source", "destination"}:
            raise PackageError("package documentation mapping has an unexpected shape")
        documentation.append(
            (
                parse_relative_path(mapping["source"], f"documentation source {index}"),
                parse_relative_path(
                    mapping["destination"], f"documentation destination {index}"
                ),
            )
        )
    project_value = decoded["project_files"]
    if not isinstance(project_value, list) or not project_value:
        raise PackageError("package project files must be a non-empty array")
    project_files = tuple(
        parse_relative_path(value, f"project file {index}")
        for index, value in enumerate(project_value)
    )
    limits_value = decoded["limits"]
    if not isinstance(limits_value, dict) or set(limits_value) != {
        "max_file_count",
        "max_file_bytes",
        "max_expanded_bytes",
        "max_encoded_bytes",
        "max_manifest_bytes",
    }:
        raise PackageError("package limits have an unexpected shape")
    limits = PackageLimits(
        max_file_count=parse_positive_limit(
            limits_value["max_file_count"], "maximum file count"
        ),
        max_file_bytes=parse_positive_limit(
            limits_value["max_file_bytes"], "maximum file bytes"
        ),
        max_expanded_bytes=parse_positive_limit(
            limits_value["max_expanded_bytes"], "maximum expanded bytes"
        ),
        max_encoded_bytes=parse_positive_limit(
            limits_value["max_encoded_bytes"], "maximum encoded bytes"
        ),
        max_manifest_bytes=parse_positive_limit(
            limits_value["max_manifest_bytes"], "maximum manifest bytes"
        ),
    )
    destinations = [destination for _, destination in documentation]
    destinations.extend(f"project/{relative}" for relative in project_files)
    folded_destinations = [destination.casefold() for destination in destinations]
    if len(destinations) != len(set(destinations)) or len(folded_destinations) != len(
        set(folded_destinations)
    ):
        raise PackageError("package layout has duplicate or case-colliding destinations")
    if len(destinations) + 4 > limits.max_file_count:
        raise PackageError("package layout cannot fit within its file-count limit")
    return Layout(
        package_root=package_root,
        documentation=tuple(documentation),
        project_files=project_files,
        limits=limits,
    )


def package_destinations(layout: Layout, platform: str) -> tuple[str, ...]:
    suffix = PLATFORM_SUFFIXES[platform]
    destinations = [destination for _, destination in layout.documentation]
    destinations.extend(f"project/{relative}" for relative in layout.project_files)
    destinations.extend(
        (
            f"bin/headless{suffix}",
            f"bin/desktop{suffix}",
            f"tools/desktop-render-probe{suffix}",
        )
    )
    return tuple(sorted(destinations))


def validate_platform(value: object) -> str:
    if not isinstance(value, str) or value not in PLATFORM_SUFFIXES:
        raise PackageError("platform is unsupported")
    return value


def validate_version(value: object) -> str:
    if (
        not isinstance(value, str)
        or len(value) > 128
        or not VERSION_PATTERN.fullmatch(value)
    ):
        raise PackageError("version must use a bounded semantic-version form")
    return value


def validate_revision(value: object) -> str:
    if not isinstance(value, str) or not REVISION_PATTERN.fullmatch(value):
        raise PackageError("source revision must be a lowercase 40-character SHA-1")
    return value


def normalized_repository_root(value: str) -> Path:
    root = Path(value)
    require_directory(root, "repository root")
    try:
        return root.resolve(strict=True)
    except OSError as error:
        raise PackageError("repository root could not be resolved") from error


def normalized_output_path(value: str, repository_root: Path) -> Path:
    output = Path(value)
    if output.suffix.lower() != ARCHIVE_SUFFIX:
        raise PackageError("candidate output must use the fixed .zip archive format")
    if os.path.lexists(output):
        raise PackageError("candidate output must not already exist")
    parent = output.parent
    require_directory(parent, "candidate output parent")
    try:
        resolved_parent = parent.resolve(strict=True)
    except OSError as error:
        raise PackageError("candidate output parent could not be resolved") from error
    output = resolved_parent / output.name
    if path_is_within(output, repository_root):
        raise PackageError("candidate output must be outside the repository root")
    return output


def normalized_new_file_path(
    value: str, repository_root: Path, subject: str, suffix: str
) -> Path:
    output = Path(value)
    if output.suffix.lower() != suffix:
        raise PackageError(f"{subject} must use the fixed {suffix} suffix")
    if os.path.lexists(output):
        raise PackageError(f"{subject} must not already exist")
    parent = output.parent
    require_directory(parent, f"{subject} parent")
    try:
        resolved_parent = parent.resolve(strict=True)
    except OSError as error:
        raise PackageError(f"{subject} parent could not be resolved") from error
    output = resolved_parent / output.name
    if path_is_within(output, repository_root):
        raise PackageError(f"{subject} must be outside the repository root")
    return output


def normalized_new_directory_path(value: str, repository_root: Path, subject: str) -> Path:
    output = Path(value)
    if os.path.lexists(output):
        raise PackageError(f"{subject} must not already exist")
    parent = output.parent
    require_directory(parent, f"{subject} parent")
    try:
        resolved_parent = parent.resolve(strict=True)
    except OSError as error:
        raise PackageError(f"{subject} parent could not be resolved") from error
    output = resolved_parent / output.name
    if path_is_within(output, repository_root):
        raise PackageError(f"{subject} must be outside the repository root")
    return output


def source_files(
    layout: Layout,
    repository_root: Path,
    platform: str,
    headless_binary: Path,
    desktop_binary: Path,
    desktop_probe_binary: Path,
) -> tuple[StagedFile, ...]:
    suffix = PLATFORM_SUFFIXES[platform]
    for binary, subject in (
        (headless_binary, "headless binary"),
        (desktop_binary, "desktop binary"),
        (desktop_probe_binary, "desktop probe binary"),
    ):
        require_regular_file(binary, subject)
    staged: list[StagedFile] = []
    for source, destination in layout.documentation:
        staged.append(StagedFile(repository_root / source, destination, False))
    project_root = repository_root / "reference-game"
    require_directory(project_root, "reference-game project root")
    for relative in layout.project_files:
        staged.append(StagedFile(project_root / relative, f"project/{relative}", False))
    staged.extend(
        (
            StagedFile(headless_binary, f"bin/headless{suffix}", True),
            StagedFile(desktop_binary, f"bin/desktop{suffix}", True),
            StagedFile(desktop_probe_binary, f"tools/desktop-render-probe{suffix}", True),
        )
    )
    destinations = [entry.destination for entry in staged]
    if tuple(sorted(destinations)) != package_destinations(layout, platform):
        raise PackageError("candidate source map does not match the fixed package layout")
    for entry in staged:
        require_regular_file(entry.source, f"candidate source {entry.destination}")
        try:
            size = entry.source.stat().st_size
        except OSError as error:
            raise PackageError("candidate source size could not be read") from error
        if size > layout.limits.max_file_bytes:
            raise PackageError("candidate source exceeds the per-file byte limit")
    return tuple(sorted(staged, key=lambda entry: entry.destination))


def copy_file(source: Path, destination: Path, maximum_bytes: int) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    try:
        with source.open("rb") as input_file, destination.open("xb") as output_file:
            copied = 0
            while chunk := input_file.read(CHUNK_SIZE):
                copied += len(chunk)
                if copied > maximum_bytes:
                    raise PackageError("candidate staging copy exceeds its byte limit")
                output_file.write(chunk)
    except OSError as error:
        raise PackageError("candidate staging copy failed") from error


def sha256_and_size(stream: BinaryIO) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    while chunk := stream.read(CHUNK_SIZE):
        digest.update(chunk)
        size += len(chunk)
    return digest.hexdigest(), size


def describe_file(path: Path, destination: str, executable: bool) -> FileDescriptor:
    require_regular_file(path, "staged candidate file")
    try:
        with path.open("rb") as stream:
            digest, size = sha256_and_size(stream)
    except OSError as error:
        raise PackageError("staged candidate file could not be read") from error
    return FileDescriptor(destination, size, digest, executable)


def validate_stage(
    stage_root: Path, expected: Iterable[StagedFile], limits: PackageLimits
) -> tuple[FileDescriptor, ...]:
    expected_entries = tuple(expected)
    expected_map = {entry.destination: entry for entry in expected_entries}
    actual_paths: list[str] = []
    for candidate in stage_root.rglob("*"):
        relative = candidate.relative_to(stage_root).as_posix()
        if path_has_link_or_reparse_point(candidate):
            raise PackageError("candidate staging tree contains a link or reparse point")
        try:
            metadata = candidate.stat()
        except OSError as error:
            raise PackageError("candidate staging tree could not be inspected") from error
        if stat.S_ISDIR(metadata.st_mode):
            continue
        if not stat.S_ISREG(metadata.st_mode):
            raise PackageError("candidate staging tree contains a non-regular file")
        actual_paths.append(relative)
    if set(actual_paths) != set(expected_map):
        raise PackageError("candidate staging tree contains missing or unexpected files")
    if len(actual_paths) > limits.max_file_count:
        raise PackageError("candidate staging tree exceeds the file-count limit")
    descriptors = tuple(
        describe_file(stage_root / destination, destination, expected_map[destination].executable)
        for destination in sorted(actual_paths)
    )
    if any(descriptor.size_bytes > limits.max_file_bytes for descriptor in descriptors):
        raise PackageError("candidate staging tree exceeds the per-file byte limit")
    total = sum(descriptor.size_bytes for descriptor in descriptors)
    if total > limits.max_expanded_bytes:
        raise PackageError("candidate staging tree exceeds the expanded-byte limit")
    return descriptors


def build_manifest(
    layout: Layout,
    platform: str,
    version: str,
    source_revision: str,
    descriptors: Iterable[FileDescriptor],
) -> bytes:
    entries = tuple(descriptors)
    payload = {
        "schema": PACKAGE_MANIFEST_SCHEMA,
        "format_version": FORMAT_VERSION,
        "package": {
            "name": "nara-reference-game",
            "version": version,
            "platform": platform,
        },
        "source_revision": source_revision,
        "layout": {
            "root": layout.package_root,
            "headless": next(
                descriptor.path for descriptor in entries if descriptor.path.startswith("bin/headless")
            ),
            "desktop": next(
                descriptor.path for descriptor in entries if descriptor.path.startswith("bin/desktop")
            ),
            "desktop_probe": next(
                descriptor.path
                for descriptor in entries
                if descriptor.path.startswith("tools/desktop-render-probe")
            ),
            "project": "project",
        },
        "limits": {
            "file_count": len(entries),
            "expanded_bytes": sum(entry.size_bytes for entry in entries),
        },
        "files": [
            {
                "path": entry.path,
                "size_bytes": entry.size_bytes,
                "sha256": entry.sha256,
                "executable": entry.executable,
            }
            for entry in entries
        ],
    }
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"
    if len(encoded) > layout.limits.max_manifest_bytes:
        raise PackageError("candidate manifest exceeds the byte limit")
    return encoded


def zip_info(path: str, executable: bool) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(path, ZIP_TIMESTAMP)
    info.create_system = 3
    permissions = 0o100755 if executable else 0o100644
    info.external_attr = permissions << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def write_archive(
    output: Path,
    stage_root: Path,
    layout: Layout,
    descriptors: Iterable[FileDescriptor],
    manifest: bytes,
) -> None:
    try:
        with zipfile.ZipFile(
            output,
            mode="x",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=9,
            strict_timestamps=True,
        ) as archive:
            for descriptor in descriptors:
                archive_path = f"{layout.package_root}/{descriptor.path}"
                with (stage_root / descriptor.path).open("rb") as source, archive.open(
                    zip_info(archive_path, descriptor.executable), mode="w"
                ) as destination:
                    shutil.copyfileobj(source, destination, CHUNK_SIZE)
            archive.writestr(
                zip_info(f"{layout.package_root}/{MANIFEST_NAME}", False), manifest
            )
    except (OSError, zipfile.BadZipFile) as error:
        raise PackageError("candidate archive could not be written") from error


def remove_owned_output(path: Path) -> None:
    try:
        if path.exists() and not path_has_link_or_reparse_point(path):
            path.unlink()
    except OSError:
        pass


def file_sha256(path: Path) -> str:
    try:
        with path.open("rb") as stream:
            digest, _ = sha256_and_size(stream)
    except OSError as error:
        raise PackageError("candidate archive could not be read") from error
    return digest


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"


def write_new_file(path: Path, contents: bytes, subject: str) -> None:
    try:
        with path.open("xb") as output:
            output.write(contents)
    except OSError as error:
        raise PackageError(f"{subject} could not be written") from error


def publish_new_file(source: Path, destination: Path, subject: str) -> None:
    try:
        os.link(source, destination)
    except OSError as error:
        raise PackageError(f"{subject} could not be published without replacing a file") from error


def create_candidate(arguments: argparse.Namespace) -> dict[str, object]:
    layout = load_layout()
    platform = validate_platform(arguments.platform)
    version = validate_version(arguments.version)
    source_revision = validate_revision(arguments.source_revision)
    repository_root = normalized_repository_root(arguments.repository_root)
    output = normalized_output_path(arguments.output, repository_root)
    receipt_path = (
        normalized_new_file_path(
            arguments.receipt, repository_root, "candidate receipt", ".json"
        )
        if arguments.receipt is not None
        else None
    )
    sources = source_files(
        layout,
        repository_root,
        platform,
        Path(arguments.headless_binary),
        Path(arguments.desktop_binary),
        Path(arguments.desktop_probe_binary),
    )
    with tempfile.TemporaryDirectory(
        prefix=".nara-reference-game-package-", dir=output.parent
    ) as temporary:
        stage_root = Path(temporary) / layout.package_root
        stage_root.mkdir()
        for entry in sources:
            copy_file(
                entry.source,
                stage_root / entry.destination,
                layout.limits.max_file_bytes,
            )
        descriptors = validate_stage(stage_root, sources, layout.limits)
        manifest = build_manifest(layout, platform, version, source_revision, descriptors)
        temporary_archive = Path(temporary) / "candidate.zip"
        write_archive(temporary_archive, stage_root, layout, descriptors, manifest)
        try:
            encoded_bytes = temporary_archive.stat().st_size
        except OSError as error:
            raise PackageError("candidate archive size could not be read") from error
        if encoded_bytes > layout.limits.max_encoded_bytes:
            raise PackageError("candidate archive exceeds the encoded-byte limit")
        receipt = {
            "schema": PACKAGE_MANIFEST_SCHEMA,
            "format_version": FORMAT_VERSION,
            "platform": platform,
            "source_revision": source_revision,
            "archive": {
                "filename": output.name,
                "size_bytes": encoded_bytes,
                "sha256": file_sha256(temporary_archive),
            },
        }
        publish_new_file(temporary_archive, output, "candidate archive")
    if receipt_path is not None:
        try:
            write_new_file(receipt_path, canonical_json_bytes(receipt), "candidate receipt")
        except PackageError:
            remove_owned_output(output)
            raise
    return receipt


def load_candidate_receipt(path: Path) -> dict[str, Any]:
    raw = read_file_bounded(path, "candidate receipt", MAX_BUNDLE_RECEIPT_BYTES)
    try:
        receipt = json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as error:
        raise PackageError("candidate receipt is not valid JSON") from error
    if not isinstance(receipt, dict) or set(receipt) != {
        "schema",
        "format_version",
        "platform",
        "source_revision",
        "archive",
    }:
        raise PackageError("candidate receipt has an unexpected shape")
    if (
        receipt["schema"] != PACKAGE_MANIFEST_SCHEMA
        or receipt["format_version"] != FORMAT_VERSION
    ):
        raise PackageError("candidate receipt schema is unsupported")
    validate_platform(receipt["platform"])
    validate_revision(receipt["source_revision"])
    archive = receipt["archive"]
    if not isinstance(archive, dict) or set(archive) != {
        "filename",
        "size_bytes",
        "sha256",
    }:
        raise PackageError("candidate receipt archive identity is invalid")
    filename = archive["filename"]
    size = archive["size_bytes"]
    digest = archive["sha256"]
    if (
        not isinstance(filename, str)
        or Path(filename).name != filename
        or not filename.endswith(ARCHIVE_SUFFIX)
        or not isinstance(size, int)
        or isinstance(size, bool)
        or size <= 0
        or not isinstance(digest, str)
        or len(digest) != 64
        or not all(character in "0123456789abcdef" for character in digest)
    ):
        raise PackageError("candidate receipt archive identity is invalid")
    return receipt


def bundle_file_descriptor(root: Path, relative: str) -> dict[str, object]:
    path = root / relative
    require_regular_file(path, "candidate transport file")
    try:
        with path.open("rb") as stream:
            digest, size = sha256_and_size(stream)
    except OSError as error:
        raise PackageError("candidate transport file could not be read") from error
    return {"path": relative, "size_bytes": size, "sha256": digest}


def bundle_candidate(arguments: argparse.Namespace) -> dict[str, object]:
    repository_root = normalized_repository_root(arguments.repository_root)
    layout = load_layout()
    archive = Path(arguments.archive)
    receipt_path = Path(arguments.receipt)
    require_regular_file(archive, "candidate archive")
    receipt = load_candidate_receipt(receipt_path)
    try:
        archive = archive.resolve(strict=True)
    except OSError as error:
        raise PackageError("candidate archive could not be resolved") from error
    archive_identity = receipt["archive"]
    if archive.name != archive_identity["filename"]:
        raise PackageError("candidate archive filename does not match its receipt")
    try:
        archive_size = archive.stat().st_size
    except OSError as error:
        raise PackageError("candidate archive size could not be read") from error
    if (
        archive_size > layout.limits.max_encoded_bytes
        or archive_size != archive_identity["size_bytes"]
        or file_sha256(archive) != archive_identity["sha256"]
    ):
        raise PackageError("candidate archive does not match its receipt")
    output = normalized_new_directory_path(
        arguments.output, repository_root, "candidate transport bundle"
    )
    helper_sources = (
        (
            tool_root() / "package.py",
            "verification/reference-game/tools/package.py",
        ),
        (
            tool_root() / "smoke_artifact.py",
            "verification/reference-game/tools/smoke_artifact.py",
        ),
        (
            layout_path(),
            "verification/reference-game/packaging/package-layout-v1.json",
        ),
    )
    for source, _ in helper_sources:
        require_regular_file(source, "candidate verification helper")
    with tempfile.TemporaryDirectory(
        prefix=".nara-candidate-bundle-", dir=output.parent
    ) as temporary:
        temporary_root = Path(temporary)
        archive_relative = f"candidate/{archive.name}"
        receipt_relative = "candidate/receipt.json"
        copy_file(
            archive,
            temporary_root / archive_relative,
            layout.limits.max_encoded_bytes,
        )
        copy_file(
            receipt_path,
            temporary_root / receipt_relative,
            MAX_BUNDLE_RECEIPT_BYTES,
        )
        for source, relative in helper_sources:
            limit = (
                MAX_JSON_BYTES
                if relative.endswith("package-layout-v1.json")
                else MAX_BUNDLE_HELPER_BYTES
            )
            copy_file(source, temporary_root / relative, limit)
        relative_files = tuple(
            sorted(
                (
                    archive_relative,
                    receipt_relative,
                    *(relative for _, relative in helper_sources),
                )
            )
        )
        descriptors = tuple(
            bundle_file_descriptor(temporary_root, relative) for relative in relative_files
        )
        bundle_manifest = {
            "schema": BUNDLE_MANIFEST_SCHEMA,
            "format_version": FORMAT_VERSION,
            "platform": receipt["platform"],
            "source_revision": receipt["source_revision"],
            "archive": {
                "path": archive_relative,
                **archive_identity,
            },
            "receipt": receipt_relative,
            "files": descriptors,
        }
        write_new_file(
            temporary_root / "bundle-manifest.json",
            canonical_json_bytes(bundle_manifest),
            "candidate transport manifest",
        )
        try:
            os.replace(temporary_root, output)
        except OSError as error:
            raise PackageError("candidate transport bundle could not be published") from error
    return {
        "schema": BUNDLE_MANIFEST_SCHEMA,
        "format_version": FORMAT_VERSION,
        "platform": receipt["platform"],
        "source_revision": receipt["source_revision"],
        "archive": archive_identity,
    }


def parse_arguments(arguments: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("create", help="create one fixed candidate ZIP")
    create.add_argument("--repository-root", required=True)
    create.add_argument("--platform", required=True)
    create.add_argument("--version", required=True)
    create.add_argument("--source-revision", required=True)
    create.add_argument("--headless-binary", required=True)
    create.add_argument("--desktop-binary", required=True)
    create.add_argument("--desktop-probe-binary", required=True)
    create.add_argument("--output", required=True)
    create.add_argument("--receipt")
    bundle = commands.add_parser(
        "bundle", help="create one fixed no-checkout consumer transport directory"
    )
    bundle.add_argument("--repository-root", required=True)
    bundle.add_argument("--archive", required=True)
    bundle.add_argument("--receipt", required=True)
    bundle.add_argument("--output", required=True)
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    try:
        parsed = parse_arguments(sys.argv[1:] if arguments is None else arguments)
        if parsed.command == "create":
            receipt = create_candidate(parsed)
        elif parsed.command == "bundle":
            receipt = bundle_candidate(parsed)
        else:
            raise PackageError("package command is unsupported")
    except PackageError as error:
        print(f"package: {error}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
