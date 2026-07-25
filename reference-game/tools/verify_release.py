#!/usr/bin/env python3
"""Build one bounded publication manifest from a pinned pre-release approval.

The workflow supplies the approval record and independently collected GitHub facts as canonical
files. This tool never fetches, extracts, executes, uploads, or deletes candidate content.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
from typing import Any, NoReturn, Sequence


APPROVAL_SCHEMA = "nara.reference-game.pre-release-approval-v1"
TRUSTED_INPUT_SCHEMA = "nara.reference-game.release-trusted-input-v1"
MANIFEST_SCHEMA = "nara.reference-game.publication-manifest-v1"
FORMAT_VERSION = 1
MAX_APPROVAL_BYTES = 128 * 1024
MAX_TRUSTED_INPUT_BYTES = 128 * 1024
MAX_MANIFEST_BYTES = 64 * 1024
MAX_DEPTH = 32
MAX_NODES = 4_096
MAX_CONTAINER_ITEMS = 64
MAX_STRING_BYTES = 4_096
MAX_TOTAL_STRING_BYTES = 64 * 1024
MIN_RETENTION_SECONDS = 24 * 60 * 60
REVISION_PATTERN = re.compile(r"^[0-9a-f]{40}$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
VERSION_PATTERN = re.compile(
    r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
IDENTIFIER_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
REPOSITORY_PATTERN = re.compile(r"^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}$")
PLATFORMS = ("linux-x86_64", "windows-x86_64")
APPROVAL_FIELDS = (
    "schema",
    "format_version",
    "decision",
    "version",
    "repository",
    "source_revision",
    "protocol_sha256",
    "publisher",
    "normalized_evidence",
    "journey",
    "review",
    "next_slice",
    "release_notes",
    "candidates",
)
MANIFEST_FIELDS = (
    "schema",
    "format_version",
    "approval",
    "repository",
    "source_revision",
    "protocol_sha256",
    "version",
    "tag",
    "publisher",
    "release",
    "candidates",
    "checksum_file",
)
TRUSTED_INPUT_FIELDS = (
    "schema",
    "format_version",
    "now_unix_seconds",
    "approval",
    "repository",
    "publisher",
    "tag",
    "candidates",
)


class ReleaseVerificationError(Exception):
    """The release publication input is unsafe, incomplete, or substituted."""


def reject() -> NoReturn:
    raise ReleaseVerificationError()


def canonical_json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode("utf-8")


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def path_has_link_or_reparse_point(path: Path) -> bool:
    try:
        metadata = path.lstat()
    except OSError:
        reject()
    attributes = getattr(metadata, "st_file_attributes", 0)
    reparse_point = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return stat.S_ISLNK(metadata.st_mode) or bool(attributes & reparse_point)


def link_free_absolute(path: Path) -> Path:
    try:
        absolute = path.absolute()
        current = Path(absolute.anchor)
        for part in absolute.parts[1:]:
            if part in ("", ".", ".."):
                reject()
            current /= part
            if path_has_link_or_reparse_point(current):
                reject()
        return absolute
    except (OSError, RuntimeError):
        reject()


def resolve_regular_file(path: Path, maximum_bytes: int) -> Path:
    if maximum_bytes <= 0:
        reject()
    try:
        absolute = link_free_absolute(path)
        metadata = absolute.stat()
        if not stat.S_ISREG(metadata.st_mode):
            reject()
        if metadata.st_size > maximum_bytes:
            reject()
        return absolute.resolve(strict=True)
    except (OSError, RuntimeError):
        reject()


def resolve_new_file(path: Path) -> Path:
    try:
        absolute = path.absolute()
        if (
            not absolute.name
            or absolute.name in (".", "..")
            or os.path.lexists(absolute)
        ):
            reject()
        parent = link_free_absolute(absolute.parent)
        metadata = parent.stat()
        if not stat.S_ISDIR(metadata.st_mode):
            reject()
        return parent.resolve(strict=True) / absolute.name
    except (OSError, RuntimeError):
        reject()


def read_bounded(path: Path, maximum_bytes: int) -> bytes:
    try:
        chunks: list[bytes] = []
        remaining = maximum_bytes
        with path.open("rb") as source:
            while True:
                chunk = source.read(min(64 * 1024, remaining + 1))
                if not chunk:
                    break
                if len(chunk) > remaining:
                    reject()
                chunks.append(chunk)
                remaining -= len(chunk)
        return b"".join(chunks)
    except (OSError, ValueError):
        reject()


def scan_json_depth(encoded: bytes) -> None:
    depth = 0
    in_string = False
    escaped = False
    for byte in encoded:
        if in_string:
            if escaped:
                escaped = False
            elif byte == ord("\\"):
                escaped = True
            elif byte == ord('"'):
                in_string = False
            continue
        if byte == ord('"'):
            in_string = True
        elif byte in (ord("{"), ord("[")):
            depth += 1
            if depth > MAX_DEPTH:
                reject()
        elif byte in (ord("}"), ord("]")):
            depth -= 1
            if depth < 0:
                reject()
    if in_string or depth != 0:
        reject()


def validate_json_shape(root: Any) -> None:
    nodes = 0
    total_string_bytes = 0
    stack: list[tuple[Any, int]] = [(root, 1)]
    while stack:
        value, depth = stack.pop()
        nodes += 1
        if nodes > MAX_NODES or depth > MAX_DEPTH:
            reject()
        if isinstance(value, dict):
            if len(value) > MAX_CONTAINER_ITEMS:
                reject()
            for key, nested in value.items():
                total_string_bytes = validate_string_budget(key, total_string_bytes)
                stack.append((nested, depth + 1))
        elif isinstance(value, list):
            if len(value) > MAX_CONTAINER_ITEMS:
                reject()
            stack.extend((nested, depth + 1) for nested in value)
        elif isinstance(value, str):
            total_string_bytes = validate_string_budget(value, total_string_bytes)
        elif value is None or isinstance(value, bool) or isinstance(value, int):
            continue
        else:
            reject()


def validate_string_budget(value: str, total: int) -> int:
    length = len(value.encode("utf-8"))
    if length > MAX_STRING_BYTES or total + length > MAX_TOTAL_STRING_BYTES:
        reject()
    return total + length


def parse_json(encoded: bytes) -> Any:
    scan_json_depth(encoded)

    def duplicate_free_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                reject()
            result[key] = value
        return result

    def parse_integer(token: str) -> int:
        if len(token) > 20:
            reject()
        try:
            return int(token)
        except ValueError:
            reject()

    def reject_float(_: str) -> NoReturn:
        reject()

    try:
        decoded = encoded.decode("utf-8")
        value = json.loads(
            decoded,
            object_pairs_hook=duplicate_free_object,
            parse_int=parse_integer,
            parse_float=reject_float,
            parse_constant=reject_float,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError, ValueError):
        reject()
    validate_json_shape(value)
    return value


def load_canonical(path: Path, maximum_bytes: int) -> tuple[Any, bytes]:
    source = resolve_regular_file(path, maximum_bytes)
    encoded = read_bounded(source, maximum_bytes)
    value = parse_json(encoded)
    if canonical_json_bytes(value) != encoded:
        reject()
    return value, encoded


def mapping(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        reject()
    return value


def exact_keys(value: dict[str, Any], expected: tuple[str, ...]) -> None:
    if tuple(value) != expected:
        reject()


def validate_schema(value: Any, schema_id: str, fields: tuple[str, ...]) -> None:
    schema = mapping(value)
    exact_keys(
        schema,
        (
            "$schema",
            "$id",
            "title",
            "type",
            "additionalProperties",
            "required",
            "properties",
        ),
    )
    properties = mapping(schema["properties"])
    if (
        schema["$schema"] != "https://json-schema.org/draft/2020-12/schema"
        or schema["$id"] != schema_id
        or schema["type"] != "object"
        or schema["additionalProperties"] is not False
        or schema["required"] != list(fields)
        or tuple(properties) != fields
    ):
        reject()
    schema_property = mapping(properties["schema"])
    format_property = mapping(properties["format_version"])
    exact_keys(schema_property, ("const",))
    exact_keys(format_property, ("const",))
    if schema_property["const"] != schema_id or format_property["const"] != FORMAT_VERSION:
        reject()


def string(value: Any, maximum_bytes: int = 512) -> str:
    if not isinstance(value, str) or not value:
        reject()
    if len(value.encode("utf-8")) > maximum_bytes or "\x00" in value:
        reject()
    return value


def single_line(value: Any, maximum_bytes: int = 512) -> str:
    rendered = string(value, maximum_bytes)
    if "\r" in rendered or "\n" in rendered:
        reject()
    return rendered


def unsigned(value: Any, *, nonzero: bool = False) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        reject()
    if nonzero and value == 0:
        reject()
    return value


def decimal_identifier(value: Any) -> str:
    rendered = string(value, 20)
    if not rendered.isdecimal() or set(rendered) == {"0"}:
        reject()
    return rendered


def identifier(value: Any) -> str:
    rendered = string(value, 128)
    if not IDENTIFIER_PATTERN.fullmatch(rendered):
        reject()
    return rendered


def revision(value: Any) -> str:
    rendered = string(value, 40)
    if not REVISION_PATTERN.fullmatch(rendered):
        reject()
    return rendered


def digest(value: Any) -> str:
    rendered = string(value, 64)
    if not SHA256_PATTERN.fullmatch(rendered):
        reject()
    return rendered


def artifact_digest(value: Any) -> str:
    rendered = string(value, 71)
    if not rendered.startswith("sha256:") or not SHA256_PATTERN.fullmatch(rendered[7:]):
        reject()
    return rendered


def safe_repository(value: Any) -> str:
    rendered = string(value, 201)
    if not REPOSITORY_PATTERN.fullmatch(rendered):
        reject()
    return rendered


def safe_relative_path(value: Any) -> str:
    rendered = string(value, 512)
    if (
        not rendered.isascii()
        or "\\" in rendered
        or ":" in rendered
        or rendered.startswith("/")
        or rendered.endswith("/")
    ):
        reject()
    path = PurePosixPath(rendered)
    if (
        path.as_posix() != rendered
        or any(part in ("", ".", "..") for part in path.parts)
        or any(len(part) > 255 for part in path.parts)
    ):
        reject()
    return rendered


def version(value: Any) -> str:
    rendered = string(value, 128)
    if not VERSION_PATTERN.fullmatch(rendered):
        reject()
    return rendered


def candidate_archive_name(platform: str) -> str:
    return f"nara-reference-game-{platform}.zip"


def candidate_artifact_name(platform: str, run_id: str, run_attempt: int) -> str:
    return f"nara-reference-game-{platform}-{run_id}-{run_attempt}"


def validate_approval_candidate(value: Any, repository: str, source_revision: str) -> dict[str, Any]:
    candidate = mapping(value)
    exact_keys(candidate, ("platform", "artifact", "archive", "workflow"))
    platform = string(candidate["platform"], 32)
    if platform not in PLATFORMS:
        reject()

    artifact = mapping(candidate["artifact"])
    exact_keys(artifact, ("id", "name", "size_bytes", "sha256", "expires_at_unix_seconds"))
    artifact_id = decimal_identifier(artifact["id"])
    artifact_name = single_line(artifact["name"], 192)
    artifact_bytes = unsigned(artifact["size_bytes"], nonzero=True)
    artifact_sha256 = artifact_digest(artifact["sha256"])
    expires_at = unsigned(artifact["expires_at_unix_seconds"], nonzero=True)

    archive = mapping(candidate["archive"])
    exact_keys(archive, ("path", "filename", "size_bytes", "sha256"))
    archive_path = safe_relative_path(archive["path"])
    archive_name = single_line(archive["filename"], 160)
    archive_bytes = unsigned(archive["size_bytes"], nonzero=True)
    archive_sha256 = digest(archive["sha256"])
    if (
        archive_name != candidate_archive_name(platform)
        or archive_path != f"candidate/{archive_name}"
    ):
        reject()

    workflow = mapping(candidate["workflow"])
    exact_keys(
        workflow,
        (
            "repository",
            "path",
            "definition_sha256",
            "event",
            "ref",
            "source_revision",
            "run_id",
            "run_attempt",
        ),
    )
    workflow_repository = safe_repository(workflow["repository"])
    workflow_path = safe_relative_path(workflow["path"])
    workflow_sha256 = digest(workflow["definition_sha256"])
    workflow_event = string(workflow["event"], 64)
    workflow_ref = string(workflow["ref"], 128)
    workflow_source = revision(workflow["source_revision"])
    workflow_run_id = decimal_identifier(workflow["run_id"])
    workflow_attempt = unsigned(workflow["run_attempt"], nonzero=True)
    if (
        workflow_repository != repository
        or workflow_path != ".github/workflows/reference-game-candidate.yml"
        or workflow_event != "workflow_dispatch"
        or workflow_ref != "refs/heads/main"
        or workflow_source != source_revision
        or artifact_name != candidate_artifact_name(platform, workflow_run_id, workflow_attempt)
    ):
        reject()
    return {
        "platform": platform,
        "artifact": {
            "id": artifact_id,
            "name": artifact_name,
            "size_bytes": artifact_bytes,
            "sha256": artifact_sha256,
            "expires_at_unix_seconds": expires_at,
        },
        "archive": {
            "path": archive_path,
            "filename": archive_name,
            "size_bytes": archive_bytes,
            "sha256": archive_sha256,
        },
        "workflow": {
            "repository": workflow_repository,
            "path": workflow_path,
            "definition_sha256": workflow_sha256,
            "event": workflow_event,
            "ref": workflow_ref,
            "source_revision": workflow_source,
            "run_id": workflow_run_id,
            "run_attempt": workflow_attempt,
        },
    }


def validate_release_notes(value: Any) -> dict[str, Any]:
    notes = mapping(value)
    exact_keys(
        notes,
        (
            "title",
            "audience",
            "supported_slice",
            "deferred_capabilities",
            "breaking_change_policy",
        ),
    )
    title = single_line(notes["title"], 256)
    audience = single_line(notes["audience"], 1_024)
    breaking_change_policy = single_line(notes["breaking_change_policy"], 1_024)
    supported = notes["supported_slice"]
    deferred = notes["deferred_capabilities"]
    if (
        not isinstance(supported, list)
        or not isinstance(deferred, list)
        or not supported
        or not deferred
        or len(supported) > 16
        or len(deferred) > 16
    ):
        reject()
    supported_slice = [single_line(item, 512) for item in supported]
    deferred_capabilities = [single_line(item, 512) for item in deferred]
    if len(set(supported_slice)) != len(supported_slice) or len(set(deferred_capabilities)) != len(
        deferred_capabilities
    ):
        reject()
    return {
        "title": title,
        "audience": audience,
        "supported_slice": supported_slice,
        "deferred_capabilities": deferred_capabilities,
        "breaking_change_policy": breaking_change_policy,
    }


def validate_approval(value: Any) -> dict[str, Any]:
    approval = mapping(value)
    exact_keys(approval, APPROVAL_FIELDS)
    if approval["schema"] != APPROVAL_SCHEMA or approval["format_version"] != FORMAT_VERSION:
        reject()
    if approval["decision"] != "Publish":
        reject()
    release_version = version(approval["version"])
    repository = safe_repository(approval["repository"])
    source_revision = revision(approval["source_revision"])
    protocol_sha256 = digest(approval["protocol_sha256"])

    publisher = mapping(approval["publisher"])
    exact_keys(publisher, ("workflow_path", "definition_sha256", "source_revision"))
    publisher_path = safe_relative_path(publisher["workflow_path"])
    publisher_sha256 = digest(publisher["definition_sha256"])
    publisher_source = revision(publisher["source_revision"])
    if publisher_path != ".github/workflows/reference-game-release.yml":
        reject()

    normalized_evidence = mapping(approval["normalized_evidence"])
    exact_keys(normalized_evidence, ("path", "size_bytes", "sha256"))
    evidence_path = safe_relative_path(normalized_evidence["path"])
    evidence_size = unsigned(normalized_evidence["size_bytes"], nonzero=True)
    evidence_sha256 = digest(normalized_evidence["sha256"])
    if not evidence_path.startswith("docs/benchmarks/data/"):
        reject()

    journey = mapping(approval["journey"])
    exact_keys(journey, ("id", "runner_class", "result", "sha256"))
    journey_id = identifier(journey["id"])
    journey_runner = string(journey["runner_class"], 32)
    journey_result = string(journey["result"], 32)
    journey_sha256 = digest(journey["sha256"])
    if journey_runner not in ("human", "coding-agent") or journey_result != "passed":
        reject()

    review = mapping(approval["review"])
    exact_keys(review, ("id", "result", "unresolved_p0", "unresolved_p1", "sha256"))
    review_id = identifier(review["id"])
    review_result = string(review["result"], 32)
    review_p0 = unsigned(review["unresolved_p0"])
    review_p1 = unsigned(review["unresolved_p1"])
    review_sha256 = digest(review["sha256"])
    if review_result != "cleared" or review_p0 != 0 or review_p1 != 0:
        reject()

    next_slice = mapping(approval["next_slice"])
    exact_keys(next_slice, ("decision", "rule", "sha256"))
    next_slice_decision = string(next_slice["decision"], 32)
    next_slice_rule = single_line(next_slice["rule"], 1_024)
    next_slice_sha256 = digest(next_slice["sha256"])
    if next_slice_decision not in ("deferred", "trial-admitted"):
        reject()

    candidates_value = approval["candidates"]
    if not isinstance(candidates_value, list) or len(candidates_value) != len(PLATFORMS):
        reject()
    candidates = [
        validate_approval_candidate(candidate, repository, source_revision)
        for candidate in candidates_value
    ]
    if tuple(candidate["platform"] for candidate in candidates) != PLATFORMS:
        reject()

    return {
        "version": release_version,
        "repository": repository,
        "source_revision": source_revision,
        "protocol_sha256": protocol_sha256,
        "publisher": {
            "workflow_path": publisher_path,
            "definition_sha256": publisher_sha256,
            "source_revision": publisher_source,
        },
        "normalized_evidence": {
            "path": evidence_path,
            "size_bytes": evidence_size,
            "sha256": evidence_sha256,
        },
        "journey": {
            "id": journey_id,
            "runner_class": journey_runner,
            "result": journey_result,
            "sha256": journey_sha256,
        },
        "review": {
            "id": review_id,
            "result": review_result,
            "unresolved_p0": review_p0,
            "unresolved_p1": review_p1,
            "sha256": review_sha256,
        },
        "next_slice": {
            "decision": next_slice_decision,
            "rule": next_slice_rule,
            "sha256": next_slice_sha256,
        },
        "release_notes": validate_release_notes(approval["release_notes"]),
        "candidates": candidates,
    }


def validate_trusted_candidate(value: Any, now: int) -> dict[str, Any]:
    candidate = mapping(value)
    exact_keys(candidate, ("platform", "artifact", "archive"))
    platform = string(candidate["platform"], 32)
    if platform not in PLATFORMS:
        reject()

    artifact = mapping(candidate["artifact"])
    exact_keys(
        artifact,
        (
            "id",
            "name",
            "size_bytes",
            "sha256",
            "expires_at_unix_seconds",
            "expired",
            "workflow_run",
        ),
    )
    artifact_id = decimal_identifier(artifact["id"])
    artifact_name = single_line(artifact["name"], 192)
    artifact_size = unsigned(artifact["size_bytes"], nonzero=True)
    artifact_sha256 = artifact_digest(artifact["sha256"])
    expires_at = unsigned(artifact["expires_at_unix_seconds"], nonzero=True)
    if artifact["expired"] is not False or expires_at <= now + MIN_RETENTION_SECONDS:
        reject()

    workflow_run = mapping(artifact["workflow_run"])
    exact_keys(
        workflow_run,
        (
            "id",
            "repository",
            "path",
            "definition_sha256",
            "event",
            "ref",
            "source_revision",
            "run_attempt",
        ),
    )
    run_id = decimal_identifier(workflow_run["id"])
    run_repository = safe_repository(workflow_run["repository"])
    run_path = safe_relative_path(workflow_run["path"])
    run_definition_sha256 = digest(workflow_run["definition_sha256"])
    run_event = string(workflow_run["event"], 64)
    run_ref = string(workflow_run["ref"], 128)
    run_source = revision(workflow_run["source_revision"])
    run_attempt = unsigned(workflow_run["run_attempt"], nonzero=True)
    if (
        artifact_name != candidate_artifact_name(platform, run_id, run_attempt)
        or run_path != ".github/workflows/reference-game-candidate.yml"
        or run_event != "workflow_dispatch"
        or run_ref != "refs/heads/main"
    ):
        reject()

    archive = mapping(candidate["archive"])
    exact_keys(archive, ("path", "filename", "size_bytes", "sha256"))
    archive_path = safe_relative_path(archive["path"])
    archive_filename = single_line(archive["filename"], 160)
    archive_size = unsigned(archive["size_bytes"], nonzero=True)
    archive_sha256 = digest(archive["sha256"])
    if (
        archive_filename != candidate_archive_name(platform)
        or archive_path != f"candidate/{archive_filename}"
    ):
        reject()
    return {
        "platform": platform,
        "artifact": {
            "id": artifact_id,
            "name": artifact_name,
            "size_bytes": artifact_size,
            "sha256": artifact_sha256,
            "expires_at_unix_seconds": expires_at,
            "workflow_run": {
                "id": run_id,
                "repository": run_repository,
                "path": run_path,
                "definition_sha256": run_definition_sha256,
                "event": run_event,
                "ref": run_ref,
                "source_revision": run_source,
                "run_attempt": run_attempt,
            },
        },
        "archive": {
            "path": archive_path,
            "filename": archive_filename,
            "size_bytes": archive_size,
            "sha256": archive_sha256,
        },
    }


def validate_trusted_input(value: Any) -> dict[str, Any]:
    trusted = mapping(value)
    exact_keys(
        trusted,
        (
            "schema",
            "format_version",
            "now_unix_seconds",
            "approval",
            "repository",
            "publisher",
            "tag",
            "candidates",
        ),
    )
    if trusted["schema"] != TRUSTED_INPUT_SCHEMA or trusted["format_version"] != FORMAT_VERSION:
        reject()
    now = unsigned(trusted["now_unix_seconds"], nonzero=True)

    approval = mapping(trusted["approval"])
    exact_keys(approval, ("path", "commit", "blob", "sha256"))
    approval_path = safe_relative_path(approval["path"])
    approval_commit = revision(approval["commit"])
    approval_blob = revision(approval["blob"])
    approval_sha256 = digest(approval["sha256"])
    if (
        not approval_path.startswith("docs/benchmarks/data/approvals/v1/")
        or not approval_path.endswith(".json")
    ):
        reject()

    repository = mapping(trusted["repository"])
    exact_keys(
        repository,
        (
            "full_name",
            "default_branch",
            "protected_default_branch",
            "immutable_releases_enabled",
        ),
    )
    full_name = safe_repository(repository["full_name"])
    default_branch = string(repository["default_branch"], 128)
    if (
        default_branch != "main"
        or repository["protected_default_branch"] is not True
        or repository["immutable_releases_enabled"] is not True
    ):
        reject()

    publisher = mapping(trusted["publisher"])
    exact_keys(
        publisher,
        (
            "workflow_path",
            "definition_sha256",
            "event",
            "ref",
            "source_revision",
            "run_id",
            "run_attempt",
        ),
    )
    publisher_path = safe_relative_path(publisher["workflow_path"])
    publisher_sha256 = digest(publisher["definition_sha256"])
    publisher_event = string(publisher["event"], 64)
    publisher_ref = string(publisher["ref"], 128)
    publisher_source = revision(publisher["source_revision"])
    publisher_run_id = decimal_identifier(publisher["run_id"])
    publisher_run_attempt = unsigned(publisher["run_attempt"], nonzero=True)
    if (
        publisher_path != ".github/workflows/reference-game-release.yml"
        or publisher_event != "workflow_dispatch"
        or publisher_ref != "refs/heads/main"
    ):
        reject()

    tag = mapping(trusted["tag"])
    exact_keys(
        tag,
        (
            "name",
            "ref",
            "annotated",
            "protected",
            "object_type",
            "object_sha",
            "target_type",
            "target_sha",
        ),
    )
    tag_name = string(tag["name"], 160)
    tag_ref = string(tag["ref"], 192)
    tag_object_type = string(tag["object_type"], 16)
    tag_object_sha = revision(tag["object_sha"])
    tag_target_type = string(tag["target_type"], 16)
    tag_target_sha = revision(tag["target_sha"])
    if (
        not tag_name.startswith("v")
        or tag_ref != f"refs/tags/{tag_name}"
        or tag["annotated"] is not True
        or tag["protected"] is not True
        or tag_object_type != "tag"
        or tag_target_type != "commit"
    ):
        reject()

    candidates_value = trusted["candidates"]
    if not isinstance(candidates_value, list) or len(candidates_value) != len(PLATFORMS):
        reject()
    candidates = [validate_trusted_candidate(candidate, now) for candidate in candidates_value]
    if tuple(candidate["platform"] for candidate in candidates) != PLATFORMS:
        reject()

    return {
        "now_unix_seconds": now,
        "approval": {
            "path": approval_path,
            "commit": approval_commit,
            "blob": approval_blob,
            "sha256": approval_sha256,
        },
        "repository": {"full_name": full_name},
        "publisher": {
            "workflow_path": publisher_path,
            "definition_sha256": publisher_sha256,
            "source_revision": publisher_source,
            "run_id": publisher_run_id,
            "run_attempt": publisher_run_attempt,
        },
        "tag": {
            "name": tag_name,
            "ref": tag_ref,
            "object_sha": tag_object_sha,
            "target_sha": tag_target_sha,
        },
        "candidates": candidates,
    }


def validate_against_trusted(
    approval: dict[str, Any], approval_bytes: bytes, trusted: dict[str, Any]
) -> None:
    if sha256(approval_bytes) != trusted["approval"]["sha256"]:
        reject()
    if (
        approval["repository"] != trusted["repository"]["full_name"]
        or approval["source_revision"] != trusted["tag"]["target_sha"]
        or trusted["tag"]["name"] != f"v{approval['version']}"
        or approval["publisher"]
        != {
            key: trusted["publisher"][key]
            for key in ("workflow_path", "definition_sha256", "source_revision")
        }
    ):
        reject()
    for approved, observed in zip(approval["candidates"], trusted["candidates"], strict=True):
        if approved["platform"] != observed["platform"]:
            reject()
        if approved["artifact"] != {
            key: observed["artifact"][key]
            for key in ("id", "name", "size_bytes", "sha256", "expires_at_unix_seconds")
        }:
            reject()
        if approved["archive"] != observed["archive"]:
            reject()
        expected_workflow = approved["workflow"]
        observed_workflow = observed["artifact"]["workflow_run"]
        if expected_workflow != {
            "repository": observed_workflow["repository"],
            "path": observed_workflow["path"],
            "definition_sha256": observed_workflow["definition_sha256"],
            "event": observed_workflow["event"],
            "ref": observed_workflow["ref"],
            "source_revision": observed_workflow["source_revision"],
            "run_id": observed_workflow["id"],
            "run_attempt": observed_workflow["run_attempt"],
        }:
            reject()
        if (
            observed_workflow["repository"] != approval["repository"]
            or observed_workflow["source_revision"] != approval["source_revision"]
        ):
            reject()


def release_body(notes: dict[str, Any]) -> str:
    supported = "\n".join(f"- {item}" for item in notes["supported_slice"])
    deferred = "\n".join(f"- {item}" for item in notes["deferred_capabilities"])
    return (
        f"## {notes['title']}\n\n"
        f"Audience: {notes['audience']}\n\n"
        f"Supported slice:\n{supported}\n\n"
        f"Deferred capabilities:\n{deferred}\n\n"
        f"Breaking-change policy: {notes['breaking_change_policy']}\n"
    )


def checksum_file(candidates: Sequence[dict[str, Any]]) -> dict[str, Any]:
    content = "".join(
        f"{candidate['archive']['sha256']}  {candidate['archive']['filename']}\n"
        for candidate in candidates
    )
    encoded = content.encode("ascii")
    return {
        "name": "SHA256SUMS",
        "content": content,
        "size_bytes": len(encoded),
        "sha256": sha256(encoded),
    }


def build_manifest(options: argparse.Namespace) -> dict[str, Any]:
    approval_value, approval_bytes = load_canonical(Path(options.approval), MAX_APPROVAL_BYTES)
    trusted_value, _ = load_canonical(Path(options.trusted_input), MAX_TRUSTED_INPUT_BYTES)
    approval = validate_approval(approval_value)
    trusted = validate_trusted_input(trusted_value)
    validate_against_trusted(approval, approval_bytes, trusted)

    manifest = {
        "schema": MANIFEST_SCHEMA,
        "format_version": FORMAT_VERSION,
        "approval": trusted["approval"],
        "repository": approval["repository"],
        "source_revision": approval["source_revision"],
        "protocol_sha256": approval["protocol_sha256"],
        "version": approval["version"],
        "tag": trusted["tag"],
        "publisher": trusted["publisher"],
        "release": {
            "name": f"Nara Reference Game {approval['version']} Evidence Build",
            "body": release_body(approval["release_notes"]),
            "draft": True,
            "prerelease": True,
            "make_latest": False,
        },
        "candidates": [
            {
                "platform": candidate["platform"],
                "artifact": candidate["artifact"],
                "archive": candidate["archive"],
            }
            for candidate in approval["candidates"]
        ],
        "checksum_file": checksum_file(approval["candidates"]),
    }
    encoded = canonical_json_bytes(manifest)
    if len(encoded) > MAX_MANIFEST_BYTES:
        reject()
    output = resolve_new_file(Path(options.output))
    try:
        with output.open("xb") as destination:
            destination.write(encoded)
    except OSError:
        reject()
    return {
        "schema": MANIFEST_SCHEMA,
        "status": "manifest_written",
        "manifest_sha256": sha256(encoded),
        "manifest_bytes": len(encoded),
    }


def verify_policy(options: argparse.Namespace) -> dict[str, Any]:
    approval_schema, approval_schema_bytes = load_canonical(
        Path(options.approval_schema), MAX_APPROVAL_BYTES
    )
    manifest_schema, manifest_schema_bytes = load_canonical(
        Path(options.manifest_schema), MAX_APPROVAL_BYTES
    )
    trusted_schema, trusted_schema_bytes = load_canonical(
        Path(options.trusted_input_schema), MAX_APPROVAL_BYTES
    )
    validate_schema(approval_schema, APPROVAL_SCHEMA, APPROVAL_FIELDS)
    validate_schema(manifest_schema, MANIFEST_SCHEMA, MANIFEST_FIELDS)
    validate_schema(trusted_schema, TRUSTED_INPUT_SCHEMA, TRUSTED_INPUT_FIELDS)
    return {
        "schema": MANIFEST_SCHEMA,
        "status": "policy_valid",
        "approval_schema": APPROVAL_SCHEMA,
        "approval_schema_sha256": sha256(approval_schema_bytes),
        "trusted_input_schema": TRUSTED_INPUT_SCHEMA,
        "trusted_input_schema_sha256": sha256(trusted_schema_bytes),
        "manifest_schema_sha256": sha256(manifest_schema_bytes),
        "max_manifest_bytes": MAX_MANIFEST_BYTES,
        "minimum_retention_seconds": MIN_RETENTION_SECONDS,
    }


def parser() -> argparse.ArgumentParser:
    parsed = argparse.ArgumentParser(description=__doc__)
    commands = parsed.add_subparsers(dest="command", required=True)
    verify = commands.add_parser(
        "verify-policy", help="validate the pinned approval and publication schemas"
    )
    verify.add_argument("--approval-schema", required=True)
    verify.add_argument("--manifest-schema", required=True)
    verify.add_argument("--trusted-input-schema", required=True)
    build = commands.add_parser(
        "build-manifest",
        help="verify one pinned Publish approval and construct its publication manifest",
    )
    build.add_argument("--approval", required=True)
    build.add_argument("--trusted-input", required=True)
    build.add_argument("--output", required=True)
    return parsed


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "verify-policy":
            result = verify_policy(options)
        elif options.command == "build-manifest":
            result = build_manifest(options)
        else:
            raise AssertionError(f"unsupported release verifier command: {options.command}")
    except ReleaseVerificationError:
        print("release-verifier: release input rejected", file=sys.stderr)
        return 2
    print(canonical_json_bytes(result).decode("utf-8"), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
