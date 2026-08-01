#!/usr/bin/env python3
"""Preflight, extract, and smoke one fixed reference-game candidate archive."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any, BinaryIO, Sequence
import zipfile

import package


HEADLESS_SUMMARY_SCHEMA = "nara-reference-game.wave-summary-v1"
DESKTOP_SMOKE_SUCCESS = b"desktop_candidate_smoke: ok\n"
MAX_PROCESS_OUTPUT_BYTES = 64 * 1024
MAX_PROCESS_TIMEOUT_SECONDS = 60
ARTIFACT_TEMPORARY_PREFIX = ".nara-artifact-"
SMOKE_TEMPORARY_PREFIX = ".nara-smoke-"


class ArtifactError(Exception):
    """A candidate archive is unsafe, incomplete, or failed its bounded smoke."""


@dataclass(frozen=True)
class ValidatedArchive:
    archive_sha256: str
    archive_size_bytes: int
    layout: package.Layout
    manifest: dict[str, Any]
    descriptors: dict[str, dict[str, Any]]


def fail_from_package(error: package.PackageError) -> ArtifactError:
    return ArtifactError(str(error))


def require_regular_archive(path: Path, limits: package.PackageLimits) -> Path:
    try:
        package.require_regular_file(path, "candidate archive")
    except package.PackageError as error:
        raise fail_from_package(error) from error
    try:
        resolved = path.resolve(strict=True)
        size = resolved.stat().st_size
    except OSError as error:
        raise ArtifactError("candidate archive could not be inspected") from error
    if size > limits.max_encoded_bytes:
        raise ArtifactError("candidate archive exceeds the encoded-byte limit")
    return resolved


def file_sha256_and_size(path: Path, maximum_bytes: int | None = None) -> tuple[str, int]:
    try:
        with path.open("rb") as stream:
            digest = hashlib.sha256()
            size = 0
            while chunk := stream.read(package.CHUNK_SIZE):
                size += len(chunk)
                if maximum_bytes is not None and size > maximum_bytes:
                    raise ArtifactError("candidate file exceeds its byte limit")
                digest.update(chunk)
            return digest.hexdigest(), size
    except OSError as error:
        raise ArtifactError("candidate file could not be read") from error


def parse_archive_relative_path(value: str, layout: package.Layout) -> str:
    if (
        not value
        or not value.isascii()
        or "\\" in value
        or ":" in value
        or "\x00" in value
        or value.endswith("/")
    ):
        raise ArtifactError("candidate archive contains an unsafe entry name")
    path = PurePosixPath(value)
    if (
        path.is_absolute()
        or path.as_posix() != value
        or len(path.parts) < 2
        or path.parts[0] != layout.package_root
    ):
        raise ArtifactError("candidate archive entry escapes the fixed package root")
    relative = "/".join(path.parts[1:])
    try:
        return package.parse_relative_path(relative, "candidate archive entry")
    except package.PackageError as error:
        raise fail_from_package(error) from error


def validate_zip_info(info: zipfile.ZipInfo, layout: package.Layout) -> str:
    relative = parse_archive_relative_path(info.filename, layout)
    if info.flag_bits & 0x1:
        raise ArtifactError("candidate archive entries must not be encrypted")
    if info.compress_type != zipfile.ZIP_DEFLATED:
        raise ArtifactError("candidate archive uses an unsupported compression method")
    if info.file_size < 0 or info.compress_size < 0:
        raise ArtifactError("candidate archive entry has an invalid size")
    mode = info.external_attr >> 16
    if stat.S_IFMT(mode) != stat.S_IFREG:
        raise ArtifactError("candidate archive contains a link or special entry")
    return relative


def read_member(archive: zipfile.ZipFile, info: zipfile.ZipInfo, limit: int) -> bytes:
    if info.file_size > limit:
        raise ArtifactError("candidate archive entry exceeds its byte limit")
    try:
        with archive.open(info, "r") as stream:
            contents = stream.read(limit + 1)
    except (OSError, zipfile.BadZipFile) as error:
        raise ArtifactError("candidate archive entry could not be read") from error
    if len(contents) != info.file_size or len(contents) > limit:
        raise ArtifactError("candidate archive entry size is inconsistent")
    return contents


def digest_member(archive: zipfile.ZipFile, info: zipfile.ZipInfo, limit: int) -> tuple[str, int]:
    if info.file_size > limit:
        raise ArtifactError("candidate archive entry exceeds its byte limit")
    digest = hashlib.sha256()
    size = 0
    try:
        with archive.open(info, "r") as stream:
            while chunk := stream.read(package.CHUNK_SIZE):
                size += len(chunk)
                if size > limit:
                    raise ArtifactError("candidate archive entry size is inconsistent")
                digest.update(chunk)
    except (OSError, zipfile.BadZipFile) as error:
        raise ArtifactError("candidate archive entry could not be read") from error
    if size != info.file_size:
        raise ArtifactError("candidate archive entry size is inconsistent")
    return digest.hexdigest(), size


def validate_manifest_shape(
    manifest: object, layout: package.Layout, expected_platform: str | None, expected_revision: str | None
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    if not isinstance(manifest, dict) or set(manifest) != {
        "schema",
        "format_version",
        "package",
        "source_revision",
        "layout",
        "limits",
        "files",
    }:
        raise ArtifactError("candidate manifest has an unexpected shape")
    if manifest["schema"] != package.PACKAGE_MANIFEST_SCHEMA:
        raise ArtifactError("candidate manifest schema is unsupported")
    if manifest["format_version"] != package.FORMAT_VERSION:
        raise ArtifactError("candidate manifest format version is unsupported")
    package_value = manifest["package"]
    if not isinstance(package_value, dict) or set(package_value) != {
        "name",
        "version",
        "platform",
    }:
        raise ArtifactError("candidate manifest package identity is invalid")
    if package_value["name"] != "nara-reference-game":
        raise ArtifactError("candidate manifest package name is invalid")
    try:
        version = package.validate_version(package_value["version"])
        platform = package.validate_platform(package_value["platform"])
        revision = package.validate_revision(manifest["source_revision"])
    except package.PackageError as error:
        raise fail_from_package(error) from error
    if expected_platform is not None and platform != expected_platform:
        raise ArtifactError("candidate platform does not match the expected platform")
    if expected_revision is not None and revision != expected_revision:
        raise ArtifactError("candidate source revision does not match the expected revision")
    layout_value = manifest["layout"]
    if not isinstance(layout_value, dict) or set(layout_value) != {
        "root",
        "headless",
        "desktop",
        "project",
    }:
        raise ArtifactError("candidate manifest layout is invalid")
    suffix = package.PLATFORM_SUFFIXES[platform]
    expected_layout = {
        "root": layout.package_root,
        "headless": f"bin/headless{suffix}",
        "desktop": f"bin/desktop{suffix}",
        "project": "project",
    }
    if layout_value != expected_layout:
        raise ArtifactError("candidate manifest layout does not match the fixed package layout")
    files = manifest["files"]
    if not isinstance(files, list) or not files:
        raise ArtifactError("candidate manifest files must be a non-empty array")
    expected_paths = package.package_destinations(layout, platform)
    if len(files) != len(expected_paths):
        raise ArtifactError("candidate manifest file count is invalid")
    descriptors: dict[str, dict[str, Any]] = {}
    ordered_paths: list[str] = []
    for descriptor in files:
        if not isinstance(descriptor, dict) or set(descriptor) != {
            "path",
            "size_bytes",
            "sha256",
            "executable",
        }:
            raise ArtifactError("candidate manifest file descriptor is invalid")
        try:
            relative = package.parse_relative_path(descriptor["path"], "candidate manifest path")
        except package.PackageError as error:
            raise fail_from_package(error) from error
        size = descriptor["size_bytes"]
        digest = descriptor["sha256"]
        executable = descriptor["executable"]
        if (
            not isinstance(size, int)
            or isinstance(size, bool)
            or size < 0
            or size > layout.limits.max_file_bytes
            or not isinstance(digest, str)
            or len(digest) != 64
            or not all(character in "0123456789abcdef" for character in digest)
            or not isinstance(executable, bool)
        ):
            raise ArtifactError("candidate manifest file descriptor has invalid values")
        if relative in descriptors or relative.casefold() in {
            path.casefold() for path in descriptors
        }:
            raise ArtifactError("candidate manifest has duplicate or case-colliding paths")
        descriptors[relative] = {
            "path": relative,
            "size_bytes": size,
            "sha256": digest,
            "executable": executable,
        }
        ordered_paths.append(relative)
    if tuple(ordered_paths) != tuple(sorted(ordered_paths)):
        raise ArtifactError("candidate manifest file descriptors must use canonical path order")
    if tuple(ordered_paths) != expected_paths:
        raise ArtifactError("candidate manifest paths do not match the fixed package layout")
    expected_executables = {
        f"bin/headless{suffix}",
        f"bin/desktop{suffix}",
    }
    if any(
        descriptor["executable"] != (relative in expected_executables)
        for relative, descriptor in descriptors.items()
    ):
        raise ArtifactError("candidate manifest executable flags are invalid")
    total = sum(descriptor["size_bytes"] for descriptor in descriptors.values())
    if total > layout.limits.max_expanded_bytes:
        raise ArtifactError("candidate manifest exceeds the expanded-byte limit")
    limits = manifest["limits"]
    if limits != {"file_count": len(descriptors), "expanded_bytes": total}:
        raise ArtifactError("candidate manifest aggregate limits are invalid")
    package_value["version"] = version
    package_value["platform"] = platform
    manifest["source_revision"] = revision
    return manifest, descriptors


def validate_archive(
    archive_path: Path,
    expected_platform: str | None = None,
    expected_revision: str | None = None,
) -> ValidatedArchive:
    try:
        layout = package.load_layout()
    except package.PackageError as error:
        raise fail_from_package(error) from error
    if expected_platform is not None:
        try:
            package.validate_platform(expected_platform)
        except package.PackageError as error:
            raise fail_from_package(error) from error
    if expected_revision is not None:
        try:
            package.validate_revision(expected_revision)
        except package.PackageError as error:
            raise fail_from_package(error) from error
    archive_path = require_regular_archive(archive_path, layout.limits)
    archive_digest, archive_size = file_sha256_and_size(
        archive_path, layout.limits.max_encoded_bytes
    )
    try:
        with zipfile.ZipFile(archive_path, "r") as archive:
            if archive.comment:
                raise ArtifactError("candidate archive must not have a ZIP comment")
            infos = archive.infolist()
            if len(infos) > layout.limits.max_file_count + 1:
                raise ArtifactError("candidate archive exceeds the file-count limit")
            entries: dict[str, zipfile.ZipInfo] = {}
            folded: set[str] = set()
            for info in infos:
                relative = validate_zip_info(info, layout)
                if relative in entries or relative.casefold() in folded:
                    raise ArtifactError("candidate archive has duplicate or case-colliding entries")
                entries[relative] = info
                folded.add(relative.casefold())
            if package.MANIFEST_NAME not in entries:
                raise ArtifactError("candidate archive is missing its manifest")
            manifest_info = entries[package.MANIFEST_NAME]
            if (manifest_info.external_attr >> 16) != 0o100644:
                raise ArtifactError("candidate archive manifest mode is invalid")
            manifest_bytes = read_member(
                archive, manifest_info, layout.limits.max_manifest_bytes
            )
            try:
                raw_manifest = json.loads(manifest_bytes)
            except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as error:
                raise ArtifactError("candidate manifest is not valid JSON") from error
            manifest, descriptors = validate_manifest_shape(
                raw_manifest, layout, expected_platform, expected_revision
            )
            expected_paths = set(descriptors)
            if set(entries) != expected_paths | {package.MANIFEST_NAME}:
                raise ArtifactError("candidate archive has missing or unexpected entries")
            aggregate = 0
            for relative, descriptor in descriptors.items():
                info = entries[relative]
                if info.file_size != descriptor["size_bytes"]:
                    raise ArtifactError("candidate archive entry size does not match the manifest")
                expected_mode = 0o100755 if descriptor["executable"] else 0o100644
                if (info.external_attr >> 16) != expected_mode:
                    raise ArtifactError("candidate archive entry mode does not match the manifest")
                digest, size = digest_member(archive, info, layout.limits.max_file_bytes)
                if digest != descriptor["sha256"]:
                    raise ArtifactError("candidate archive entry digest does not match the manifest")
                aggregate += size
            if aggregate != manifest["limits"]["expanded_bytes"]:
                raise ArtifactError("candidate archive expanded-byte total is inconsistent")
    except (OSError, zipfile.BadZipFile) as error:
        raise ArtifactError("candidate archive could not be opened") from error
    return ValidatedArchive(
        archive_sha256=archive_digest,
        archive_size_bytes=archive_size,
        layout=layout,
        manifest=manifest,
        descriptors=descriptors,
    )


def receipt(validated: ValidatedArchive) -> dict[str, object]:
    package_value = validated.manifest["package"]
    return {
        "schema": package.PACKAGE_MANIFEST_SCHEMA,
        "format_version": package.FORMAT_VERSION,
        "platform": package_value["platform"],
        "version": package_value["version"],
        "source_revision": validated.manifest["source_revision"],
        "archive": {
            "size_bytes": validated.archive_size_bytes,
            "sha256": validated.archive_sha256,
        },
        "file_count": validated.manifest["limits"]["file_count"],
        "expanded_bytes": validated.manifest["limits"]["expanded_bytes"],
    }


def require_new_destination(destination: Path) -> Path:
    if os.path.lexists(destination):
        raise ArtifactError("consumer destination must not already exist")
    parent = destination.parent
    try:
        package.require_directory(parent, "consumer destination parent")
        return parent.resolve(strict=True) / destination.name
    except package.PackageError as error:
        raise fail_from_package(error) from error
    except OSError as error:
        raise ArtifactError("consumer destination parent could not be resolved") from error


def copy_member(
    archive: zipfile.ZipFile,
    info: zipfile.ZipInfo,
    destination: Path,
    expected_size: int,
    expected_digest: str | None,
    executable: bool,
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    copied = 0
    try:
        with archive.open(info, "r") as source, destination.open("xb") as target:
            while chunk := source.read(package.CHUNK_SIZE):
                copied += len(chunk)
                if copied > expected_size:
                    raise ArtifactError("candidate archive entry exceeded its validated size")
                digest.update(chunk)
                target.write(chunk)
    except (OSError, zipfile.BadZipFile) as error:
        raise ArtifactError("candidate archive extraction failed") from error
    if copied != expected_size or (expected_digest is not None and digest.hexdigest() != expected_digest):
        raise ArtifactError("candidate archive changed during extraction")
    try:
        destination.chmod(0o755 if executable else 0o644)
    except OSError as error:
        raise ArtifactError("candidate archive extraction mode could not be applied") from error


def assert_extracted_layout(root: Path, validated: ValidatedArchive) -> None:
    expected = set(validated.descriptors) | {package.MANIFEST_NAME}
    actual: set[str] = set()
    for candidate in root.rglob("*"):
        relative = candidate.relative_to(root).as_posix()
        if package.path_has_link_or_reparse_point(candidate):
            raise ArtifactError("extracted candidate contains a link or reparse point")
        try:
            metadata = candidate.stat()
        except OSError as error:
            raise ArtifactError("extracted candidate could not be inspected") from error
        if stat.S_ISDIR(metadata.st_mode):
            continue
        if not stat.S_ISREG(metadata.st_mode):
            raise ArtifactError("extracted candidate contains a special file")
        actual.add(relative)
    if actual != expected:
        raise ArtifactError("extracted candidate has missing or unexpected files")


def safe_remove_temporary(path: Path, parent: Path, expected_prefix: str) -> bool:
    try:
        resolved_parent = parent.resolve(strict=True)
        resolved_path = path.resolve(strict=True)
    except OSError:
        return False
    if (
        package.path_is_within(resolved_path, resolved_parent)
        and resolved_path != resolved_parent
        and resolved_path.name.startswith(expected_prefix)
    ):
        try:
            shutil.rmtree(resolved_path)
        except OSError:
            return False
        return True
    return False


def extract_archive(
    archive_path: Path,
    destination: Path,
    expected_platform: str | None = None,
    expected_revision: str | None = None,
) -> ValidatedArchive:
    validated = validate_archive(archive_path, expected_platform, expected_revision)
    archive_path = require_regular_archive(archive_path, validated.layout.limits)
    destination = require_new_destination(destination)
    temporary = Path(
        tempfile.mkdtemp(prefix=ARTIFACT_TEMPORARY_PREFIX, dir=destination.parent)
    )
    try:
        package_root = temporary / validated.layout.package_root
        package_root.mkdir()
        try:
            with zipfile.ZipFile(archive_path, "r") as archive:
                infos = {
                    parse_archive_relative_path(info.filename, validated.layout): info
                    for info in archive.infolist()
                }
                manifest_info = infos[package.MANIFEST_NAME]
                manifest_bytes = read_member(
                    archive, manifest_info, validated.layout.limits.max_manifest_bytes
                )
                manifest_destination = package_root / package.MANIFEST_NAME
                manifest_destination.write_bytes(manifest_bytes)
                manifest_destination.chmod(0o644)
                for relative, descriptor in validated.descriptors.items():
                    copy_member(
                        archive,
                        infos[relative],
                        package_root / relative,
                        descriptor["size_bytes"],
                        descriptor["sha256"],
                        descriptor["executable"],
                    )
        except (OSError, zipfile.BadZipFile) as error:
            raise ArtifactError("candidate archive extraction failed") from error
        assert_extracted_layout(package_root, validated)
        digest_after, size_after = file_sha256_and_size(
            archive_path, validated.layout.limits.max_encoded_bytes
        )
        if (
            digest_after != validated.archive_sha256
            or size_after != validated.archive_size_bytes
        ):
            raise ArtifactError("candidate archive changed during extraction")
        os.replace(temporary, destination)
    except Exception:
        safe_remove_temporary(
            temporary, destination.parent, ARTIFACT_TEMPORARY_PREFIX
        )
        raise
    return validated


def parse_launcher(value: str) -> list[str]:
    if len(value) > 8192:
        raise ArtifactError("desktop launcher JSON exceeds its byte limit")
    try:
        parsed = json.loads(value)
    except (json.JSONDecodeError, RecursionError) as error:
        raise ArtifactError("desktop launcher JSON is invalid") from error
    if not isinstance(parsed, list) or len(parsed) > 16 or not all(
        isinstance(part, str) and part and len(part) <= 512 for part in parsed
    ):
        raise ArtifactError("desktop launcher must be a bounded array of command arguments")
    return parsed


def parse_environment(values: Sequence[str]) -> dict[str, str]:
    environment: dict[str, str] = {}
    for value in values:
        raw_key, separator, raw_value = value.partition("=")
        key = raw_key.upper()
        if (
            not separator
            or not key
            or len(key) > 64
            or not key.isascii()
            or not key.replace("_", "A").isalnum()
            or not key[0].isalpha()
            or len(raw_value) > 1024
            or key.startswith(
                ("CARGO_", "RUST", "GIT_", "HOME", "USERPROFILE", "XDG_")
            )
            or key in {"TMP", "TEMP", "TMPDIR"}
        ):
            raise ArtifactError("desktop environment override is invalid")
        if key in environment:
            raise ArtifactError("desktop environment override is duplicated")
        environment[key] = raw_value
    return environment


def create_smoke_root(work_parent: Path) -> Path:
    try:
        package.require_directory(work_parent, "smoke work parent")
        work_parent = work_parent.resolve(strict=True)
        smoke_root = Path(
            tempfile.mkdtemp(prefix=SMOKE_TEMPORARY_PREFIX, dir=work_parent)
        ).resolve(strict=True)
    except package.PackageError as error:
        raise fail_from_package(error) from error
    except OSError as error:
        raise ArtifactError("smoke work root could not be created") from error
    if smoke_root.parent != work_parent:
        safe_remove_temporary(smoke_root, work_parent, SMOKE_TEMPORARY_PREFIX)
        raise ArtifactError("smoke work root escaped its authorized parent")
    return smoke_root


def smoke_environment(smoke_root: Path, overrides: dict[str, str]) -> tuple[dict[str, str], Path]:
    home = smoke_root / "home"
    cwd = smoke_root / "cwd"
    temporary = smoke_root / "tmp"
    home.mkdir()
    cwd.mkdir()
    temporary.mkdir()
    inherited_keys = (
        "SystemRoot",
        "WINDIR",
        "ComSpec",
        "PATHEXT",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    )
    environment = {
        key: value for key in inherited_keys if (value := os.environ.get(key)) is not None
    }
    if os.name == "nt":
        system_root = environment.get("SystemRoot") or environment.get("WINDIR")
        if system_root is None:
            raise ArtifactError("Windows system root is unavailable for candidate smoke")
        environment["PATH"] = os.pathsep.join(
            (str(Path(system_root) / "System32"), system_root)
        )
    else:
        environment["PATH"] = "/usr/bin:/bin"
    environment.update(
        {
            "HOME": str(home),
            "USERPROFILE": str(home),
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_CACHE_HOME": str(home / "cache"),
            "XDG_DATA_HOME": str(home / "data"),
            "TEMP": str(temporary),
            "TMP": str(temporary),
            "TMPDIR": str(temporary),
        }
    )
    environment.update(overrides)
    return environment, cwd


def read_bounded_process_output(
    stream: BinaryIO,
    output: bytearray,
    overflow: threading.Event,
    read_failure: threading.Event,
) -> None:
    try:
        while chunk := stream.read(8192):
            remaining = MAX_PROCESS_OUTPUT_BYTES + 1 - len(output)
            output.extend(chunk[:remaining])
            if len(output) > MAX_PROCESS_OUTPUT_BYTES or len(chunk) > remaining:
                overflow.set()
                return
    except OSError:
        read_failure.set()
    finally:
        stream.close()


def stop_bounded_process(process: subprocess.Popen[bytes]) -> None:
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        elif process.poll() is None:
            process.kill()
    except (OSError, ProcessLookupError):
        pass
    if process.poll() is not None:
        return
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired as error:
        raise ArtifactError("candidate process could not be terminated") from error


def run_bounded(
    command: Sequence[str], cwd: Path, environment: dict[str, str], subject: str
) -> bytes:
    if not command:
        raise ArtifactError(f"{subject} command is empty")
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=os.name == "posix",
        )
    except OSError as error:
        raise ArtifactError(f"{subject} could not be started") from error
    assert process.stdout is not None
    assert process.stderr is not None
    stdout = bytearray()
    stderr = bytearray()
    overflow = threading.Event()
    read_failure = threading.Event()
    readers = (
        threading.Thread(
            target=read_bounded_process_output,
            args=(process.stdout, stdout, overflow, read_failure),
            daemon=True,
        ),
        threading.Thread(
            target=read_bounded_process_output,
            args=(process.stderr, stderr, overflow, read_failure),
            daemon=True,
        ),
    )
    for reader in readers:
        reader.start()
    deadline = time.monotonic() + MAX_PROCESS_TIMEOUT_SECONDS
    timed_out = False
    while process.poll() is None:
        if overflow.wait(timeout=0.01) or read_failure.is_set():
            stop_bounded_process(process)
            break
        if time.monotonic() >= deadline:
            timed_out = True
            stop_bounded_process(process)
            break
    for reader in readers:
        reader.join(timeout=5)
    if any(reader.is_alive() for reader in readers):
        stop_bounded_process(process)
        for reader in readers:
            reader.join(timeout=1)
        if any(reader.is_alive() for reader in readers):
            raise ArtifactError(f"{subject} output readers did not terminate")
    if timed_out:
        raise ArtifactError(f"{subject} exceeded its execution time limit")
    if read_failure.is_set():
        raise ArtifactError(f"{subject} output could not be read")
    if overflow.is_set():
        raise ArtifactError(f"{subject} exceeded its output byte limit")
    if process.returncode != 0:
        raise ArtifactError(f"{subject} exited unsuccessfully")
    if stderr:
        raise ArtifactError(f"{subject} wrote unexpected standard error")
    return bytes(stdout)


def smoke_candidate(
    archive_path: Path,
    work_parent: Path,
    desktop_launcher_json: str,
    desktop_environment: Sequence[str],
    expected_platform: str | None = None,
    expected_revision: str | None = None,
) -> dict[str, object]:
    smoke_root = create_smoke_root(work_parent)
    cleanup_parent = smoke_root.parent
    try:
        result = smoke_candidate_in_root(
            archive_path,
            smoke_root,
            desktop_launcher_json,
            desktop_environment,
            expected_platform,
            expected_revision,
        )
    except Exception:
        safe_remove_temporary(smoke_root, cleanup_parent, SMOKE_TEMPORARY_PREFIX)
        raise
    if not safe_remove_temporary(
        smoke_root, cleanup_parent, SMOKE_TEMPORARY_PREFIX
    ):
        raise ArtifactError("smoke work root could not be retired")
    return result


def smoke_candidate_in_root(
    archive_path: Path,
    smoke_root: Path,
    desktop_launcher_json: str,
    desktop_environment: Sequence[str],
    expected_platform: str | None = None,
    expected_revision: str | None = None,
) -> dict[str, object]:
    destination = smoke_root / "consumer"
    validated = extract_archive(
        archive_path, destination, expected_platform, expected_revision
    )
    launcher = parse_launcher(desktop_launcher_json)
    overrides = parse_environment(desktop_environment)
    environment, cwd = smoke_environment(smoke_root, overrides)
    package_root = destination / validated.layout.package_root
    isolated_paths = {
        Path(environment["HOME"]),
        Path(environment["TEMP"]),
        Path(environment["TMP"]),
        Path(environment["TMPDIR"]),
        cwd,
    }
    if any(package.path_is_within(path, package_root) for path in isolated_paths):
        raise ArtifactError("smoke process state overlaps the extracted package")
    headless = package_root / validated.manifest["layout"]["headless"]
    headless_stdout = run_bounded(
        [str(headless), "--max-ticks", "96"], cwd, environment, "headless candidate"
    )
    try:
        summary = json.loads(headless_stdout)
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as error:
        raise ArtifactError("headless candidate did not emit its stable JSON summary") from error
    if not isinstance(summary, dict) or summary.get("schema") != HEADLESS_SUMMARY_SCHEMA:
        raise ArtifactError("headless candidate emitted an unexpected summary")
    desktop = package_root / validated.manifest["layout"]["desktop"]
    desktop_stdout = run_bounded(
        [*launcher, str(desktop), "--candidate-smoke"],
        cwd,
        environment,
        "desktop candidate",
    )
    if desktop_stdout != DESKTOP_SMOKE_SUCCESS:
        raise ArtifactError("desktop candidate did not complete its bounded product smoke")
    result = receipt(validated)
    result["headless_summary_schema"] = HEADLESS_SUMMARY_SCHEMA
    result["desktop"] = "candidate-smoke-completed"
    return result


def parse_arguments(arguments: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    verify = commands.add_parser("verify", help="verify one candidate without extracting it")
    verify.add_argument("--archive", required=True)
    verify.add_argument("--expected-platform")
    verify.add_argument("--expected-source-revision")
    extract = commands.add_parser("extract", help="verify then extract one candidate")
    extract.add_argument("--archive", required=True)
    extract.add_argument("--destination", required=True)
    smoke = commands.add_parser("smoke", help="verify, extract, and run bounded candidate probes")
    smoke.add_argument("--archive", required=True)
    smoke.add_argument("--work-root", required=True, help="safe parent for a unique smoke root")
    smoke.add_argument("--expected-platform")
    smoke.add_argument("--expected-source-revision")
    smoke.add_argument("--desktop-launcher-json", default="[]")
    smoke.add_argument("--desktop-environment", action="append", default=[])
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    try:
        parsed = parse_arguments(sys.argv[1:] if arguments is None else arguments)
        if parsed.command == "verify":
            result = receipt(
                validate_archive(
                    Path(parsed.archive), parsed.expected_platform, parsed.expected_source_revision
                )
            )
        elif parsed.command == "extract":
            result = receipt(extract_archive(Path(parsed.archive), Path(parsed.destination)))
        elif parsed.command == "smoke":
            result = smoke_candidate(
                Path(parsed.archive),
                Path(parsed.work_root),
                parsed.desktop_launcher_json,
                parsed.desktop_environment,
                parsed.expected_platform,
                parsed.expected_source_revision,
            )
        else:
            raise ArtifactError("artifact command is unsupported")
    except ArtifactError as error:
        print(f"artifact: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
