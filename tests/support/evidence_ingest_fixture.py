#!/usr/bin/env python3
"""Build bounded evidence-ingest fixtures without invoking production helpers."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import time
from typing import Any


EXPECTATION_SCHEMA = "nara.reference-game.evidence-expectations-v1"
NORMALIZER_ID = "nara_reference_game_ingest_v1"


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode("utf-8")


def sha256(bytes_value: bytes) -> str:
    return hashlib.sha256(bytes_value).hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_bytes())
    if not isinstance(value, dict):
        raise ValueError("fixture JSON must be an object")
    return value


def write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json(value))


def expectation(envelope_path: Path, schema_path: Path) -> dict[str, Any]:
    encoded = envelope_path.read_bytes()
    envelope = read_json(envelope_path)
    identity = envelope.get("identity")
    generator = envelope.get("generator")
    if not isinstance(identity, dict) or not isinstance(generator, str):
        raise ValueError("fixture envelope lacks a generator or identity")
    return {
        "schema": EXPECTATION_SCHEMA,
        "format_version": 1,
        "normalizer": {
            "id": NORMALIZER_ID,
            "schema_sha256": sha256(schema_path.read_bytes()),
            "validation_scope": "outer_transfer_and_structure_v1",
        },
        "envelope": {
            "path": "evidence/envelope.json",
            "sha256": sha256(encoded),
            "bytes": len(encoded),
            "generator": generator,
            "identity": identity,
            "environment": envelope.get("payload", {}).get("environment"),
            "context_receipts": envelope.get("payload", {}).get("context_receipts"),
            "restricted_raw_logs": envelope.get("restricted_raw_logs"),
        },
        "candidate": {
            "receipt": {
                "schema": "nara.reference-game.candidate-package-v1",
                "format_version": 1,
                "platform": "windows-x86_64",
                "source_revision": identity["source_revision"],
                "archive": {
                    "filename": "nara-reference-game-windows-x86_64.zip",
                    "size_bytes": 1024,
                    "sha256": "a" * 64,
                },
            },
            "workflow": {
                "repository": identity["repository"],
                "path": ".github/workflows/reference-game-candidate.yml",
                "definition_sha256": "b" * 64,
                "event": "workflow_dispatch",
                "ref": "refs/heads/main",
                "source_revision": identity["source_revision"],
                "run_id": "4242",
                "run_attempt": 1,
                "artifact_id": "99",
                "artifact_bytes": 2048,
                "retention_until_unix_seconds": int(time.time()) + 86_400,
            },
        },
    }


def prepare(envelope_path: Path, expected_path: Path, schema_path: Path) -> None:
    write_json(expected_path, expectation(envelope_path, schema_path))


def mutate(mode: str, envelope_path: Path, expected_path: Path, schema_path: Path, canary: str) -> None:
    if mode == "tamper-after-expectation":
        encoded = bytearray(envelope_path.read_bytes())
        if not encoded:
            raise ValueError("fixture envelope must not be empty")
        encoded[-1] ^= 1
        envelope_path.write_bytes(encoded)
        return

    envelope = read_json(envelope_path)
    if mode == "unsafe-identifier":
        envelope["payload"]["records"][0]["fields"][3]["value"] = {
            "type": "identifier",
            "value": f"{canary};Remove-Item",
        }
    elif mode == "unknown-field":
        envelope["payload"]["unknown"] = canary
    elif mode == "candidate-source-mismatch":
        expected = read_json(expected_path)
        expected["candidate"]["receipt"]["source_revision"] = "0" * 40
        write_json(expected_path, expected)
        return
    elif mode == "expected-environment-drift":
        expected = read_json(expected_path)
        expected["envelope"]["environment"][0]["value"]["value"] = "drifted_environment_v1"
        write_json(expected_path, expected)
        return
    elif mode == "payload-digest-mismatch":
        envelope["payload_digest"]["blake3"] = "0" * 64
    else:
        raise ValueError(f"unsupported fixture mutation: {mode}")
    write_json(envelope_path, envelope)
    prepare(envelope_path, expected_path, schema_path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("envelope", type=Path)
    prepare_parser.add_argument("expected", type=Path)
    prepare_parser.add_argument("schema", type=Path)
    mutate_parser = commands.add_parser("mutate")
    mutate_parser.add_argument(
        "mode",
        choices=(
            "tamper-after-expectation",
            "unsafe-identifier",
            "unknown-field",
            "candidate-source-mismatch",
            "expected-environment-drift",
            "payload-digest-mismatch",
        ),
    )
    mutate_parser.add_argument("envelope", type=Path)
    mutate_parser.add_argument("expected", type=Path)
    mutate_parser.add_argument("schema", type=Path)
    mutate_parser.add_argument("canary")
    options = parser.parse_args()
    if options.command == "prepare":
        prepare(options.envelope, options.expected, options.schema)
    else:
        mutate(options.mode, options.envelope, options.expected, options.schema, options.canary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
