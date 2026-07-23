#!/usr/bin/env python3
"""Normalize one bounded reference-game evidence transfer without executing candidate content.

The caller supplies a canonical trusted-input record for identity and candidate provenance;
build-expectation reads untrusted envelope bytes only to bind their length and SHA-256. The Rust
evidence-policy oracle remains responsible for complete U22 payload semantics before any approval
record can be accepted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time
from typing import Any, NoReturn, Sequence


EXPECTED_SCHEMA = "nara.reference-game.evidence-expectations-v1"
TRUSTED_INPUT_SCHEMA = "nara.reference-game.evidence-trusted-input-v1"
NORMALIZED_SCHEMA = "nara.reference-game.normalized-evidence-v1"
NORMALIZER_ID = "nara_reference_game_ingest_v1"
OUTER_VALIDATION_SCOPE = "outer_transfer_and_structure_v1"
CANDIDATE_PACKAGE_SCHEMA = "nara.reference-game.candidate-package-v1"
MAX_SCHEMA_BYTES = 128 * 1024
MAX_EXPECTED_BYTES = 64 * 1024
MAX_TRUSTED_INPUT_BYTES = 64 * 1024
DEFAULT_MAX_ENVELOPE_BYTES = 512 * 1024
MAX_ENVELOPE_BYTES = 512 * 1024
MAX_NORMALIZED_BYTES = 1024 * 1024
MAX_DEPTH = 32
MAX_NODES = 8 * 1024
MAX_CONTAINER_ITEMS = 1024
MAX_STRING_BYTES = 4 * 1024
MAX_TOTAL_STRING_BYTES = 128 * 1024
MAX_CONTEXT_RECEIPTS = 128
MAX_RECORDS = 512
MAX_FIELDS_PER_RECORD = 64
MAX_TOTAL_FIELDS = 4 * 1024
MAX_RAW_LOG_REFS = 64
MAX_U64 = (1 << 64) - 1
MIN_I64 = -(1 << 63)
MAX_I64 = (1 << 63) - 1
HEX_64 = re.compile(r"^[0-9a-f]{64}$")
REVISION_40 = re.compile(r"^[0-9a-f]{40}$")


class EvidenceIngestError(RuntimeError):
    """A static refusal for untrusted evidence input."""


def reject() -> NoReturn:
    raise EvidenceIngestError("evidence input rejected")


def sha256(encoded: bytes) -> str:
    return hashlib.sha256(encoded).hexdigest()


def canonical_json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode("utf-8")


def is_within(candidate: Path, parent: Path) -> bool:
    try:
        candidate.relative_to(parent)
    except ValueError:
        return False
    return True


def path_has_link_or_reparse_point(path: Path) -> bool:
    try:
        metadata = path.lstat()
    except OSError:
        reject()
    file_attributes = getattr(metadata, "st_file_attributes", 0)
    reparse_point = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return stat.S_ISLNK(metadata.st_mode) or bool(file_attributes & reparse_point)


def link_free_absolute(path: Path) -> Path:
    try:
        absolute = path.absolute()
        anchor = Path(absolute.anchor)
        current = anchor
        for part in absolute.parts[1:]:
            if part in ("", ".", ".."):
                reject()
            current /= part
            if path_has_link_or_reparse_point(current):
                reject()
        return absolute
    except (OSError, RuntimeError):
        reject()


def resolve_existing_directory(path: Path) -> Path:
    try:
        absolute = link_free_absolute(path)
        metadata = absolute.stat()
        if not stat.S_ISDIR(metadata.st_mode):
            reject()
        return absolute.resolve(strict=True)
    except (OSError, RuntimeError):
        reject()


def resolve_regular_file(path: Path) -> Path:
    try:
        absolute = link_free_absolute(path)
        metadata = absolute.stat()
        if not stat.S_ISREG(metadata.st_mode):
            reject()
        return absolute.resolve(strict=True)
    except (OSError, RuntimeError):
        reject()


def read_bounded(path: Path, maximum: int) -> bytes:
    if maximum <= 0:
        reject()
    try:
        chunks: list[bytes] = []
        remaining = maximum
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
    encoded_length = len(value.encode("utf-8"))
    if encoded_length > MAX_STRING_BYTES or total + encoded_length > MAX_TOTAL_STRING_BYTES:
        reject()
    return total + encoded_length


def mapping(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        reject()
    return value


def array(value: Any) -> list[Any]:
    if not isinstance(value, list):
        reject()
    return value


def string(value: Any) -> str:
    if not isinstance(value, str):
        reject()
    return value


def unsigned(value: Any, *, nonzero: bool = False) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0 or value > MAX_U64:
        reject()
    if nonzero and value == 0:
        reject()
    return value


def signed(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < MIN_I64 or value > MAX_I64:
        reject()
    return value


def exact_keys(value: dict[str, Any], keys: tuple[str, ...]) -> None:
    if set(value) != set(keys):
        reject()


def safe_identifier(value: Any) -> str:
    candidate = string(value)
    encoded = candidate.encode("ascii", errors="ignore")
    if (
        len(encoded) != len(candidate)
        or not encoded
        or len(encoded) > 160
        or encoded[0] == ord("/")
        or encoded[-1] == ord("/")
        or b"//" in encoded
        or any(byte not in b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-./" for byte in encoded)
        or any(segment in (b"", b".", b"..") for segment in encoded.split(b"/"))
    ):
        reject()
    return candidate


def safe_repository(value: Any) -> str:
    repository = safe_identifier(value)
    segments = repository.split("/")
    if len(segments) != 2:
        reject()
    for segment in segments:
        raw = segment.encode("ascii")
        if (
            len(raw) > 100
            or not raw[0:1].isalnum()
            or not raw[-1:].isalnum()
            or any(byte not in b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-" for byte in raw)
        ):
            reject()
    return repository


def safe_project_relative(value: Any) -> str:
    path = string(value)
    raw = path.encode("ascii", errors="ignore")
    if (
        len(raw) != len(path)
        or not raw
        or raw.startswith(b"/")
        or raw.endswith(b"/")
        or b"//" in raw
        or b"\\" in raw
        or b":" in raw
        or any(byte not in b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-./" for byte in raw)
        or any(segment in (b"", b".", b"..") for segment in raw.split(b"/"))
    ):
        reject()
    return path


def hexadecimal_64(value: Any) -> str:
    candidate = string(value)
    if not HEX_64.fullmatch(candidate):
        reject()
    return candidate


def revision(value: Any) -> str:
    candidate = string(value)
    if not REVISION_40.fullmatch(candidate):
        reject()
    return candidate


def digest(value: Any) -> dict[str, Any]:
    result = mapping(value)
    exact_keys(result, ("bytes", "blake3"))
    unsigned(result["bytes"], nonzero=True)
    hexadecimal_64(result["blake3"])
    return result


def evidence_identity(value: Any) -> dict[str, Any]:
    result = mapping(value)
    exact_keys(
        result,
        (
            "run_provider",
            "run_id",
            "run_attempt",
            "repository",
            "source_revision",
            "protocol_digest",
            "subject",
            "environment_class",
        ),
    )
    safe_identifier(result["run_provider"])
    run_id = string(result["run_id"])
    if not run_id or len(run_id) > 20 or not run_id.isdecimal() or set(run_id) == {"0"}:
        reject()
    unsigned(result["run_attempt"], nonzero=True)
    safe_repository(result["repository"])
    revision(result["source_revision"])
    digest(result["protocol_digest"])
    safe_identifier(result["subject"])
    safe_identifier(result["environment_class"])
    return result


def field_value(value: Any) -> dict[str, Any]:
    result = mapping(value)
    field_type = string(result.get("type"))
    if field_type in ("sensitive_redacted", "secret_redacted"):
        exact_keys(result, ("type",))
        return result
    exact_keys(result, ("type", "value"))
    if field_type == "identifier":
        safe_identifier(result["value"])
    elif field_type == "project_relative":
        safe_project_relative(result["value"])
    elif field_type == "u64":
        unsigned(result["value"])
    elif field_type == "i64":
        signed(result["value"])
    elif field_type == "bool":
        if not isinstance(result["value"], bool):
            reject()
    elif field_type == "digest":
        digest(result["value"])
    else:
        reject()
    return result


def evidence_field(value: Any) -> dict[str, Any]:
    result = mapping(value)
    exact_keys(result, ("key", "value"))
    safe_identifier(result["key"])
    field_value(result["value"])
    return result


def strictly_sorted_identifiers(values: list[Any], key: str) -> None:
    previous: str | None = None
    for value in values:
        item = mapping(value)
        candidate = safe_identifier(item.get(key))
        if previous is not None and candidate <= previous:
            reject()
        previous = candidate


def validate_context_receipts(value: Any) -> list[Any]:
    context_receipts = array(value)
    if len(context_receipts) > MAX_CONTEXT_RECEIPTS:
        reject()
    strictly_sorted_identifiers(context_receipts, "id")
    for receipt in context_receipts:
        item = mapping(receipt)
        exact_keys(item, ("id", "digest"))
        safe_identifier(item["id"])
        digest(item["digest"])
    return context_receipts


def validate_environment(value: Any) -> list[Any]:
    environment = array(value)
    if len(environment) > MAX_TOTAL_FIELDS:
        reject()
    strictly_sorted_identifiers(environment, "key")
    for field in environment:
        evidence_field(field)
    return environment


def validate_evidence_payload(value: Any) -> dict[str, Any]:
    result = mapping(value)
    exact_keys(result, ("context_receipts", "environment", "records"))
    validate_context_receipts(result["context_receipts"])
    environment = validate_environment(result["environment"])
    records = array(result["records"])
    if len(records) > MAX_RECORDS:
        reject()
    previous_record: tuple[str, str] | None = None
    total_fields = len(environment)
    for record in records:
        item = mapping(record)
        exact_keys(item, ("kind", "id", "fields"))
        record_key = (safe_identifier(item["kind"]), safe_identifier(item["id"]))
        if previous_record is not None and record_key <= previous_record:
            reject()
        previous_record = record_key
        fields = array(item["fields"])
        if len(fields) > MAX_FIELDS_PER_RECORD:
            reject()
        total_fields += len(fields)
        if total_fields > MAX_TOTAL_FIELDS:
            reject()
        strictly_sorted_identifiers(fields, "key")
        for field in fields:
            evidence_field(field)
    return result


def validate_raw_log_refs(value: Any) -> list[Any]:
    references = array(value)
    if len(references) > MAX_RAW_LOG_REFS:
        reject()
    strictly_sorted_identifiers(references, "artifact_id")
    for reference in references:
        item = mapping(reference)
        exact_keys(item, ("artifact_id", "digest", "retention_until_unix_seconds"))
        safe_identifier(item["artifact_id"])
        digest(item["digest"])
        unsigned(item["retention_until_unix_seconds"], nonzero=True)
    return references


def validate_envelope(value: Any, expectation: dict[str, Any], encoded: bytes) -> dict[str, Any]:
    envelope = mapping(value)
    exact_keys(
        envelope,
        (
            "kind",
            "format_version",
            "generator",
            "identity",
            "payload_digest",
            "payload",
            "restricted_raw_logs",
        ),
    )
    if envelope["kind"] != "nara.evidence" or envelope["format_version"] != 1:
        reject()
    expected_envelope = mapping(expectation["envelope"])
    payload = validate_evidence_payload(envelope["payload"])
    raw_logs = validate_raw_log_refs(envelope["restricted_raw_logs"])
    if (
        envelope["generator"] != expected_envelope["generator"]
        or envelope["identity"] != expected_envelope["identity"]
        or payload["environment"] != expected_envelope["environment"]
        or payload["context_receipts"] != expected_envelope["context_receipts"]
        or raw_logs != expected_envelope["restricted_raw_logs"]
        or len(encoded) != expected_envelope["bytes"]
        or sha256(encoded) != expected_envelope["sha256"]
    ):
        reject()
    safe_identifier(envelope["generator"])
    evidence_identity(envelope["identity"])
    digest(envelope["payload_digest"])
    if canonical_json_bytes(envelope) != encoded:
        reject()
    return envelope


def archive_filename(value: Any, platform: str) -> str:
    candidate = string(value)
    if (
        not candidate.isascii()
        or len(candidate) > 160
        or "/" in candidate
        or "\\" in candidate
        or ":" in candidate
        or candidate != f"nara-reference-game-{platform}.zip"
    ):
        reject()
    return candidate


def validate_candidate(value: Any, identity: dict[str, Any]) -> dict[str, Any]:
    candidate = mapping(value)
    exact_keys(candidate, ("receipt", "workflow"))
    receipt = mapping(candidate["receipt"])
    exact_keys(receipt, ("schema", "format_version", "platform", "source_revision", "archive"))
    if (
        receipt["schema"] != CANDIDATE_PACKAGE_SCHEMA
        or receipt["format_version"] != 1
        or receipt["platform"] not in ("windows-x86_64", "linux-x86_64")
        or revision(receipt["source_revision"]) != identity["source_revision"]
    ):
        reject()
    archive = mapping(receipt["archive"])
    exact_keys(archive, ("filename", "size_bytes", "sha256"))
    archive_filename(archive["filename"], receipt["platform"])
    unsigned(archive["size_bytes"], nonzero=True)
    hexadecimal_64(archive["sha256"])
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
            "artifact_id",
            "artifact_bytes",
            "retention_until_unix_seconds",
        ),
    )
    if (
        safe_repository(workflow["repository"]) != identity["repository"]
        or safe_project_relative(workflow["path"])
        != ".github/workflows/reference-game-candidate.yml"
        or workflow["event"] != "workflow_dispatch"
        or workflow["ref"] != "refs/heads/main"
        or revision(workflow["source_revision"]) != identity["source_revision"]
    ):
        reject()
    hexadecimal_64(workflow["definition_sha256"])
    for key in ("run_id", "artifact_id"):
        rendered = string(workflow[key])
        if not rendered or len(rendered) > 20 or not rendered.isdecimal() or set(rendered) == {"0"}:
            reject()
    unsigned(workflow["run_attempt"], nonzero=True)
    unsigned(workflow["artifact_bytes"], nonzero=True)
    retention = unsigned(workflow["retention_until_unix_seconds"], nonzero=True)
    if retention <= int(time.time()):
        reject()
    return candidate


def validate_trusted_input(value: Any) -> dict[str, Any]:
    trusted_input = mapping(value)
    exact_keys(trusted_input, ("schema", "format_version", "envelope", "candidate"))
    if (
        trusted_input["schema"] != TRUSTED_INPUT_SCHEMA
        or trusted_input["format_version"] != 1
    ):
        reject()
    envelope = mapping(trusted_input["envelope"])
    exact_keys(
        envelope,
        (
            "generator",
            "identity",
            "environment",
            "context_receipts",
            "restricted_raw_logs",
        ),
    )
    safe_identifier(envelope["generator"])
    identity = evidence_identity(envelope["identity"])
    validate_environment(envelope["environment"])
    validate_context_receipts(envelope["context_receipts"])
    validate_raw_log_refs(envelope["restricted_raw_logs"])
    validate_candidate(trusted_input["candidate"], identity)
    return trusted_input


def validate_expectation(value: Any, schema_bytes: bytes) -> dict[str, Any]:
    expectation = mapping(value)
    exact_keys(expectation, ("schema", "format_version", "normalizer", "envelope", "candidate"))
    if expectation["schema"] != EXPECTED_SCHEMA or expectation["format_version"] != 1:
        reject()
    normalizer = mapping(expectation["normalizer"])
    exact_keys(normalizer, ("id", "schema_sha256", "validation_scope"))
    if (
        normalizer["id"] != NORMALIZER_ID
        or normalizer["schema_sha256"] != sha256(schema_bytes)
        or normalizer["validation_scope"] != OUTER_VALIDATION_SCOPE
    ):
        reject()
    expected_envelope = mapping(expectation["envelope"])
    exact_keys(
        expected_envelope,
        (
            "path",
            "sha256",
            "bytes",
            "generator",
            "identity",
            "environment",
            "context_receipts",
            "restricted_raw_logs",
        ),
    )
    if safe_project_relative(expected_envelope["path"]) != "evidence/envelope.json":
        reject()
    hexadecimal_64(expected_envelope["sha256"])
    unsigned(expected_envelope["bytes"], nonzero=True)
    safe_identifier(expected_envelope["generator"])
    identity = evidence_identity(expected_envelope["identity"])
    validate_environment(expected_envelope["environment"])
    validate_context_receipts(expected_envelope["context_receipts"])
    validate_raw_log_refs(expected_envelope["restricted_raw_logs"])
    validate_candidate(expectation["candidate"], identity)
    return expectation


def validate_schema(value: Any) -> None:
    schema = mapping(value)
    exact_keys(
        schema,
        ("$schema", "$id", "title", "type", "additionalProperties", "required", "properties"),
    )
    if (
        schema["$schema"] != "https://json-schema.org/draft/2020-12/schema"
        or schema["$id"] != NORMALIZED_SCHEMA
        or schema["title"] != "Nara Reference-Game Normalized Evidence"
        or schema["type"] != "object"
        or schema["additionalProperties"] is not False
        or not isinstance(schema["required"], list)
        or set(schema["required"])
        != {"schema", "format_version", "normalizer", "input", "evidence"}
    ):
        reject()
    properties = mapping(schema["properties"])
    if set(properties) != {"schema", "format_version", "normalizer", "input", "evidence"}:
        reject()
    normalizer = mapping(properties["normalizer"])
    if (
        normalizer.get("type") != "object"
        or normalizer.get("additionalProperties") is not False
        or normalizer.get("required") != ["id", "schema_sha256", "validation_scope"]
    ):
        reject()
    normalizer_properties = mapping(normalizer.get("properties"))
    if (
        set(normalizer_properties) != {"id", "schema_sha256", "validation_scope"}
        or normalizer_properties["id"] != {"const": NORMALIZER_ID}
        or normalizer_properties["schema_sha256"]
        != {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        or normalizer_properties["validation_scope"]
        != {"const": OUTER_VALIDATION_SCOPE}
    ):
        reject()


def output_path(path: Path, sources: Sequence[Path]) -> Path:
    try:
        if path.exists() or path.is_symlink():
            reject()
        parent = resolve_existing_directory(path.parent)
        destination = (parent / path.name).resolve(strict=False)
        if destination.parent != parent or not destination.name:
            reject()
        for source in sources:
            if is_within(destination, source.parent) or is_within(source, destination.parent):
                reject()
        return destination
    except (OSError, RuntimeError):
        reject()


def publish_new_file(destination: Path, encoded: bytes) -> None:
    if len(encoded) > MAX_NORMALIZED_BYTES:
        reject()
    temporary: Path | None = None
    try:
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{destination.name}.", suffix=".tmp", dir=destination.parent
        )
        temporary = Path(temporary_name)
        with os.fdopen(descriptor, "wb") as output:
            output.write(encoded)
            output.flush()
            os.fsync(output.fileno())
        os.link(temporary, destination)
    except (FileExistsError, OSError):
        reject()
    finally:
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError:
                pass


def normalize(options: argparse.Namespace) -> dict[str, object]:
    envelope_path = resolve_regular_file(options.envelope)
    expected_path = resolve_regular_file(options.expected)
    schema_path = resolve_regular_file(options.schema)
    destination = output_path(options.output, (envelope_path, expected_path, schema_path))
    schema_bytes = read_bounded(schema_path, MAX_SCHEMA_BYTES)
    expected_bytes = read_bounded(expected_path, MAX_EXPECTED_BYTES)
    envelope_bytes = read_bounded(envelope_path, options.max_envelope_bytes)
    schema = parse_json(schema_bytes)
    validate_schema(schema)
    expectation = validate_expectation(parse_json(expected_bytes), schema_bytes)
    envelope = validate_envelope(parse_json(envelope_bytes), expectation, envelope_bytes)
    normalized: dict[str, object] = {
        "schema": NORMALIZED_SCHEMA,
        "format_version": 1,
        "normalizer": {
            "id": NORMALIZER_ID,
            "schema_sha256": sha256(schema_bytes),
            "validation_scope": OUTER_VALIDATION_SCOPE,
        },
        "input": {
            "envelope": expectation["envelope"],
            "candidate": expectation["candidate"],
        },
        "evidence": envelope,
    }
    encoded = canonical_json_bytes(normalized)
    publish_new_file(destination, encoded)
    return {
        "schema": NORMALIZED_SCHEMA,
        "normalized_sha256": sha256(encoded),
        "normalized_bytes": len(encoded),
    }


def build_expectation(options: argparse.Namespace) -> dict[str, object]:
    envelope_path = resolve_regular_file(options.envelope)
    trusted_input_path = resolve_regular_file(options.trusted_input)
    schema_path = resolve_regular_file(options.schema)
    destination = output_path(
        options.output,
        (envelope_path, trusted_input_path, schema_path),
    )
    schema_bytes = read_bounded(schema_path, MAX_SCHEMA_BYTES)
    trusted_input_bytes = read_bounded(trusted_input_path, MAX_TRUSTED_INPUT_BYTES)
    envelope_bytes = read_bounded(envelope_path, options.max_envelope_bytes)
    validate_schema(parse_json(schema_bytes))
    trusted_input = validate_trusted_input(parse_json(trusted_input_bytes))
    if canonical_json_bytes(trusted_input) != trusted_input_bytes:
        reject()
    trusted_envelope = mapping(trusted_input["envelope"])
    expectation: dict[str, object] = {
        "schema": EXPECTED_SCHEMA,
        "format_version": 1,
        "normalizer": {
            "id": NORMALIZER_ID,
            "schema_sha256": sha256(schema_bytes),
            "validation_scope": OUTER_VALIDATION_SCOPE,
        },
        "envelope": {
            "path": "evidence/envelope.json",
            "sha256": sha256(envelope_bytes),
            "bytes": len(envelope_bytes),
            "generator": trusted_envelope["generator"],
            "identity": trusted_envelope["identity"],
            "environment": trusted_envelope["environment"],
            "context_receipts": trusted_envelope["context_receipts"],
            "restricted_raw_logs": trusted_envelope["restricted_raw_logs"],
        },
        "candidate": trusted_input["candidate"],
    }
    encoded = canonical_json_bytes(expectation)
    if len(encoded) > MAX_EXPECTED_BYTES:
        reject()
    publish_new_file(destination, encoded)
    return {
        "schema": EXPECTED_SCHEMA,
        "expected_sha256": sha256(encoded),
        "expected_bytes": len(encoded),
    }


def verify_policy(options: argparse.Namespace) -> dict[str, object]:
    schema_path = resolve_regular_file(options.schema)
    schema_bytes = read_bounded(schema_path, MAX_SCHEMA_BYTES)
    validate_schema(parse_json(schema_bytes))
    return {
        "schema": NORMALIZED_SCHEMA,
        "status": "policy_valid",
        "schema_sha256": sha256(schema_bytes),
        "validation_scope": OUTER_VALIDATION_SCOPE,
    }


def positive_envelope_limit(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be an integer") from error
    if parsed <= 0 or parsed > MAX_ENVELOPE_BYTES:
        raise argparse.ArgumentTypeError("must be within the supported evidence limit")
    return parsed


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    verify = commands.add_parser("verify-policy", help="validate the pinned normalized-evidence schema")
    verify.add_argument("--schema", type=Path, required=True)
    expectation_parser = commands.add_parser(
        "build-expectation",
        help="bind trusted identity input to one untrusted transfer without accepting its contents",
    )
    expectation_parser.add_argument("--envelope", type=Path, required=True)
    expectation_parser.add_argument("--trusted-input", type=Path, required=True)
    expectation_parser.add_argument("--schema", type=Path, required=True)
    expectation_parser.add_argument("--output", type=Path, required=True)
    expectation_parser.add_argument(
        "--max-envelope-bytes",
        type=positive_envelope_limit,
        default=DEFAULT_MAX_ENVELOPE_BYTES,
    )
    normalize_parser = commands.add_parser(
        "normalize",
        help="validate one trusted outer transfer without executing candidate code",
    )
    normalize_parser.add_argument("--envelope", type=Path, required=True)
    normalize_parser.add_argument("--expected", type=Path, required=True)
    normalize_parser.add_argument("--schema", type=Path, required=True)
    normalize_parser.add_argument("--output", type=Path, required=True)
    normalize_parser.add_argument(
        "--max-envelope-bytes",
        type=positive_envelope_limit,
        default=DEFAULT_MAX_ENVELOPE_BYTES,
    )
    return parser


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "verify-policy":
            result = verify_policy(options)
        elif options.command == "build-expectation":
            result = build_expectation(options)
        elif options.command == "normalize":
            result = normalize(options)
        else:
            raise AssertionError(f"unhandled command: {options.command}")
    except EvidenceIngestError:
        print("evidence-ingest: evidence input rejected", file=sys.stderr)
        return 2
    print(canonical_json_bytes(result).decode("utf-8"), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
