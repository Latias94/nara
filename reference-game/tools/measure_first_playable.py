#!/usr/bin/env python3
"""Prepare one isolated, non-decisive first-playable measurement plan."""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
from typing import Any, Sequence


PLAN_SCHEMA = "nara.reference-game.first-playable-plan-v1"
PLAN_FILENAME = "measurement-plan.json"
MAX_GIT_OUTPUT_BYTES = 64 * 1024
MAX_METRIC_CATALOG_BYTES = 256 * 1024
MAX_HISTORICAL_RECEIPT_BYTES = 64 * 1024
MAX_HISTORICAL_FILE_BYTES = 1024 * 1024
HISTORICAL_IMPORT_SCHEMA = "nara.reference-game.first-playable-historical-import-v1"
HISTORICAL_IMPORT_STATUS = "historical_transport_preserved_u9_repair_pending"
HISTORICAL_SOURCE_REVISION = "b2ddb5b0ea6ea9b98e213619dccf65a5326b1505"
HISTORICAL_MANIFEST_NAME = "run-manifest.json"
HISTORICAL_RAW_SAMPLES_NAME = "raw-samples.jsonl"
HISTORICAL_FILE_PATHS = {
    "measurement-plan.json": (
        "measurement-plan.json",
        "measurement-plan.original.base64",
    ),
    HISTORICAL_RAW_SAMPLES_NAME: (
        HISTORICAL_RAW_SAMPLES_NAME,
        "raw-samples.original.base64",
    ),
    HISTORICAL_MANIFEST_NAME: (
        HISTORICAL_MANIFEST_NAME,
        "run-manifest.original.base64",
    ),
    "preflight-failures.jsonl": (
        "preflight-failures.jsonl",
        "preflight-failures.original.base64",
    ),
}
HISTORICAL_REQUIRED_ORIGINALS = frozenset(
    (HISTORICAL_MANIFEST_NAME, HISTORICAL_RAW_SAMPLES_NAME)
)
HISTORICAL_COLLECTOR = {
    "sha256": "c70aea68f493ca3874e4beb25ad78b4183407d0e200c925b5951e286fe899cf4",
    "committed_at_collection_time": False,
    "archived_here": False,
    "reason": (
        "The one-run collector contained host-specific absolute paths and is not a "
        "reproducible repository instruction."
    ),
}
HISTORICAL_CLAIMS = {
    "historical_samples_preserved": True,
    "historical_collector_reproducible": False,
    "rgd_u9_complete": False,
    "u11_dependency_satisfied": False,
}
GIT_INSPECTION_TIMEOUT_SECONDS = 10.0
GIT_OUTPUT_DRAIN_TIMEOUT_SECONDS = 1.0
GIT_POLL_INTERVAL_SECONDS = 0.01
CLEAN_STATUS_COMMAND = "git status --porcelain=v1 -z"
METRIC_CATALOG_RELATIVE_PATH = (
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json"
)
METRIC_COLLECTOR = "u14"

REQUIRED_RELATIVE_PATHS = (
    "Cargo.toml",
    "reference-game/Cargo.toml",
    "reference-game/Cargo.lock",
    "reference-game/src/bin/headless.rs",
    "reference-game/src/bin/desktop.rs",
    "reference-game/src/bin/desktop_render_probe.rs",
    "reference-game/src/systems.rs",
    "reference-game/scenes/startup.scene.json",
    "reference-game/tests/public_surface.rs",
    METRIC_CATALOG_RELATIVE_PATH,
)

ENVIRONMENT_FIELDS = (
    "os_name",
    "os_release",
    "runner_image",
    "rustc_version",
    "cargo_version",
    "cpu_model",
    "build_profile",
    "desktop_adapter_or_software_profile",
    "collector_revision",
)

STEPS = (
    {
        "id": "build_timings",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/Cargo.toml",
        "command": ["cargo", "build", "--locked", "--bins"],
        "isolation": "collect one cold build with an empty detached-worktree target and one warm incremental build with its retained target",
        "metric_ids": ["build.cold_ns", "build.incremental_ns"],
        "success": "both public binary builds succeed with monotonic command-boundary samples",
    },
    {
        "id": "clean_headless_wave",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/src/bin/headless.rs",
        "command": [
            "cargo",
            "run",
            "--locked",
            "--bin",
            "headless",
            "--",
            "--max-ticks",
            "96",
        ],
        "metric_ids": [
            "journey.clean_to_headless_wave_ns",
            "gameplay.headless_wave_success",
        ],
        "success": "one terminal nara-reference-game.wave-summary-v1 JSON record",
    },
    {
        "id": "body_edit_reload",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/src/systems.rs",
        "mutation": "apply and revert one reviewed compatible Rust body-only edit in the isolated worktree",
        "metric_ids": ["iteration.body.p50_ns", "iteration.body.p95_ns"],
        "success": "a fresh rebuilt headless runtime reaches one terminal wave after the body edit",
    },
    {
        "id": "data_edit_reload",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/scenes/startup.scene.json",
        "mutation": "apply and revert one reviewed data-only change in the isolated worktree",
        "metric_ids": ["iteration.data.p50_ns", "iteration.data.p95_ns"],
        "success": "the headless public entry point reaches one terminal wave after the data change",
    },
    {
        "id": "structural_rust_edit",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/src/systems.rs",
        "mutation": "apply and revert one reviewed behavior-neutral Rust source edit in the isolated worktree",
        "metric_ids": [
            "iteration.structural.p50_ns",
            "iteration.structural.p95_ns",
        ],
        "success": "a fresh rebuilt headless runtime reaches one terminal wave",
    },
    {
        "id": "desktop_manual_playthrough",
        "kind": "manual",
        "working_directory": "reference-game",
        "entry_point": "reference-game/src/bin/desktop.rs",
        "command": [
            "cargo",
            "run",
            "--locked",
            "--features",
            "desktop",
            "--bin",
            "desktop",
        ],
        "metric_ids": [
            "journey.clean_to_desktop_playable_ns",
            "gameplay.desktop_playable_success",
        ],
        "success": "a human confirms visible movement, HUD updates, terminal state, Enter retry, and normal close",
    },
    {
        "id": "public_production_coverage",
        "kind": "automatic",
        "working_directory": "reference-game",
        "entry_point": "reference-game/tests/public_surface.rs",
        "command": ["cargo", "test", "--locked", "--test", "public_surface"],
        "metric_ids": ["public.production.coverage_basis_points"],
        "success": "the documented public surface succeeds without a private measurement hook",
    },
)

BLOCKED_WORKFLOWS = (
    {
        "id": "module_addition",
        "kind": "blocked",
        "metric_ids": ["module.add.time_ns", "module.add.success"],
        "reason": "The canonical protocol names this public task, but the current reference-game documentation has no committed module-addition task definition or public completion command.",
    },
    {
        "id": "window_slot_configuration",
        "kind": "blocked",
        "metric_ids": ["slot.configure.time_ns", "slot.configure.success"],
        "reason": "The canonical protocol names this public task, but the current reference-game documentation has no committed window-slot configuration task definition or public completion command.",
    },
    {
        "id": "desktop_pressure_telemetry",
        "kind": "blocked",
        "metric_ids": [
            "frame.p99_ns",
            "runtime.gpu_resource_bytes",
            "runtime.memory_bytes",
        ],
        "reason": "The public desktop product does not yet expose the bounded telemetry required by these canonical metrics.",
    },
)

RAW_SAMPLE_FIELDS = (
    "metric_id",
    "sample_index",
    "sample_value",
    "value_unit",
    "population",
    "mechanism",
    "start_boundary",
    "end_boundary",
    "started_at_utc",
    "completed_at_utc",
    "exit_status",
    "environment_fingerprint",
    "source_revision",
    "failure_output_reference",
)

UNAVAILABLE_MEASUREMENTS = (
    {
        "id": "frame.p99_ns",
        "reason": "The public desktop product emits no bounded frame-time sample stream.",
    },
    {
        "id": "runtime.memory_bytes",
        "reason": "No cross-platform public process-memory collector is selected for this product path.",
    },
    {
        "id": "runtime.gpu_resource_bytes",
        "reason": "Backend cache statistics are not exposed by the public desktop product entry point.",
    },
    {
        "id": "render.packet.batch_count",
        "reason": "Sprite and UI batch statistics are not exported by the public desktop product entry point.",
    },
    {
        "id": "render.packet.instance_count",
        "reason": "Sprite and UI instance counts are not exported by the public desktop product entry point.",
    },
    {
        "id": "render.packet.retained_bytes",
        "reason": "The current RenderFramePacket is topology-only and does not publish retained payload bytes.",
    },
    {
        "id": "render.packet.clone_bytes",
        "reason": "The current renderer does not publish packet clone or allocation counters.",
    },
)


class MeasurementError(RuntimeError):
    """A stable, actionable refusal while preparing local measurement work."""


def read_bounded(path: Path, limit: int, purpose: str) -> bytes:
    try:
        with path.open("rb") as source:
            encoded = source.read(limit + 1)
    except OSError as error:
        raise MeasurementError(f"The {purpose} could not be read: {error}") from error
    if len(encoded) > limit:
        raise MeasurementError(f"The {purpose} exceeded its bounded byte limit.")
    return encoded


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def historical_file(archive: Path, name: object, purpose: str) -> Path:
    if not isinstance(name, str) or not name or Path(name).name != name:
        raise MeasurementError(f"The historical {purpose} path is invalid.")
    path = resolve_path(archive / name, f"historical {purpose}")
    if not is_within(path, archive):
        raise MeasurementError(f"The historical {purpose} escaped its archive.")
    return path


def verify_historical_archive(archive_argument: Path) -> None:
    archive = resolve_path(archive_argument, "historical measurement archive")
    try:
        if not archive.is_dir():
            raise MeasurementError("The historical measurement archive must be a directory.")
    except OSError as error:
        raise MeasurementError(f"The historical archive could not be inspected: {error}") from error

    receipt_bytes = read_bounded(
        archive / "import-receipt.json",
        MAX_HISTORICAL_RECEIPT_BYTES,
        "historical import receipt",
    )
    try:
        receipt = json.loads(receipt_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError("The historical import receipt is invalid JSON.") from error
    if not isinstance(receipt, dict):
        raise MeasurementError("The historical import receipt is not an object.")
    files = receipt.get("files")
    if (
        set(receipt) != {
            "schema",
            "source_revision",
            "status",
            "collector",
            "files",
            "claims",
        }
        or receipt.get("schema") != HISTORICAL_IMPORT_SCHEMA
        or receipt.get("source_revision") != HISTORICAL_SOURCE_REVISION
        or receipt.get("status") != HISTORICAL_IMPORT_STATUS
        or receipt.get("collector") != HISTORICAL_COLLECTOR
        or receipt.get("claims") != HISTORICAL_CLAIMS
        or not isinstance(files, list)
        or len(files) != len(HISTORICAL_FILE_PATHS)
    ):
        raise MeasurementError("The historical import receipt has an invalid contract.")

    original_by_name: dict[str, bytes] = {}
    seen: set[str] = set()
    for entry in files:
        if not isinstance(entry, dict):
            raise MeasurementError("The historical file entry is invalid.")
        logical_name = entry.get("logical_name")
        if (
            set(entry)
            != {
                "logical_name",
                "semantic_copy",
                "semantic_copy_sha256",
                "original_transport_base64",
                "original_sha256",
            }
            or not isinstance(logical_name, str)
            or logical_name in seen
            or logical_name not in HISTORICAL_FILE_PATHS
            or (
                entry.get("semantic_copy"),
                entry.get("original_transport_base64"),
            )
            != HISTORICAL_FILE_PATHS[logical_name]
        ):
            raise MeasurementError("The historical file identity is invalid or duplicated.")
        seen.add(logical_name)
        semantic_path = historical_file(archive, entry.get("semantic_copy"), "semantic copy")
        transport_path = historical_file(
            archive,
            entry.get("original_transport_base64"),
            "transport copy",
        )
        semantic = read_bounded(semantic_path, MAX_HISTORICAL_FILE_BYTES, "semantic copy")
        if sha256(semantic) != entry.get("semantic_copy_sha256"):
            raise MeasurementError("A historical semantic copy digest did not match.")
        encoded = read_bounded(
            transport_path,
            MAX_HISTORICAL_FILE_BYTES * 2,
            "Base64 transport copy",
        )
        try:
            original = base64.b64decode(b"".join(encoded.split()), validate=True)
        except (ValueError, binascii.Error) as error:
            raise MeasurementError("A historical Base64 transport is invalid.") from error
        if len(original) > MAX_HISTORICAL_FILE_BYTES:
            raise MeasurementError("A decoded historical transport exceeded its byte limit.")
        if sha256(original) != entry.get("original_sha256"):
            raise MeasurementError("A historical transport digest did not match.")
        try:
            normalized_original = (
                original.decode("utf-8")
                .replace("\r\n", "\n")
                .replace("\r", "\n")
                .encode("utf-8")
            )
        except UnicodeDecodeError as error:
            raise MeasurementError(
                "A historical transport is not valid UTF-8 text."
            ) from error
        if semantic != normalized_original:
            raise MeasurementError(
                "A historical semantic copy diverged from its original transport."
            )
        if logical_name in HISTORICAL_REQUIRED_ORIGINALS:
            original_by_name[logical_name] = original

    if seen != frozenset(HISTORICAL_FILE_PATHS):
        raise MeasurementError("The historical file set is incomplete.")

    try:
        manifest = json.loads(original_by_name[HISTORICAL_MANIFEST_NAME])
        raw = original_by_name[HISTORICAL_RAW_SAMPLES_NAME]
    except (KeyError, UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError("The historical run manifest or raw samples are invalid.") from error
    if manifest.get("raw_samples_sha256") != sha256(raw):
        raise MeasurementError("The historical manifest does not bind the raw sample bytes.")
    raw_sample_count = sum(
        1 for line in io.BytesIO(raw) if line.rstrip(b"\r\n")
    )
    if manifest.get("raw_sample_count") != raw_sample_count:
        raise MeasurementError("The historical manifest does not bind the raw sample count.")
    for line in io.BytesIO(raw):
        line = line.rstrip(b"\r\n")
        if not line:
            continue
        try:
            record = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
            raise MeasurementError("A historical raw sample is invalid JSON.") from error
        if not isinstance(record, dict):
            raise MeasurementError("A historical raw sample is not an object.")


class BoundedOutputReader(threading.Thread):
    """Drain one combined subprocess stream without admitting unbounded output."""

    def __init__(self, stream: Any) -> None:
        super().__init__(daemon=True)
        self._stream = stream
        self.output = bytearray()
        self.overflowed = False
        self.error: Exception | None = None

    def run(self) -> None:
        try:
            while True:
                chunk = self._stream.read1(8 * 1024)
                if not chunk:
                    return
                remaining = MAX_GIT_OUTPUT_BYTES - len(self.output)
                if len(chunk) > remaining:
                    self.overflowed = True
                    return
                self.output.extend(chunk)
        except (OSError, ValueError) as error:
            self.error = error


def is_within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def resolve_path(path: Path, purpose: str) -> Path:
    try:
        return path.resolve()
    except (OSError, RuntimeError) as error:
        raise MeasurementError(f"The {purpose} path could not be resolved: {error}") from error


def finish_output_reader(process: subprocess.Popen[bytes], reader: BoundedOutputReader) -> None:
    reader.join(GIT_OUTPUT_DRAIN_TIMEOUT_SECONDS)
    if not reader.is_alive():
        return

    assert process.stdout is not None
    try:
        process.stdout.close()
    except OSError:
        pass
    reader.join(GIT_OUTPUT_DRAIN_TIMEOUT_SECONDS)
    if reader.is_alive():
        raise MeasurementError("Git output did not finish draining within its bounded deadline.")


def abort_git_process(
    process: subprocess.Popen[bytes], reader: BoundedOutputReader, reason: str
) -> None:
    if process.poll() is None:
        try:
            process.kill()
        except OSError as error:
            raise MeasurementError(f"Git subject inspection could not be stopped: {error}") from error
    try:
        process.wait(timeout=GIT_OUTPUT_DRAIN_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired as error:
        raise MeasurementError("Git subject inspection did not stop within its bounded deadline.") from error
    finish_output_reader(process, reader)
    raise MeasurementError(reason)


def run_git(subject: Path, arguments: Sequence[str]) -> bytes:
    environment = {
        key: value for key, value in os.environ.items() if not key.upper().startswith("GIT_")
    }
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    try:
        process = subprocess.Popen(
            ["git", "-C", os.fspath(subject), *arguments],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=environment,
        )
    except OSError as error:
        raise MeasurementError(f"Git subject inspection could not start: {error}") from error
    if process.stdout is None:
        raise MeasurementError("Git subject inspection did not provide a readable output stream.")
    reader = BoundedOutputReader(process.stdout)
    reader.start()
    deadline = time.monotonic() + GIT_INSPECTION_TIMEOUT_SECONDS
    while process.poll() is None:
        if reader.error is not None:
            abort_git_process(
                process,
                reader,
                f"Git output could not be read: {reader.error}",
            )
        if reader.overflowed:
            abort_git_process(
                process,
                reader,
                "Git output exceeded the bounded measurement-subject limit.",
            )
        if time.monotonic() >= deadline:
            abort_git_process(
                process,
                reader,
                "Git subject inspection exceeded its bounded deadline.",
            )
        time.sleep(GIT_POLL_INTERVAL_SECONDS)
    finish_output_reader(process, reader)
    if reader.error is not None:
        raise MeasurementError(f"Git output could not be read: {reader.error}") from reader.error
    if reader.overflowed:
        raise MeasurementError("Git output exceeded the bounded measurement-subject limit.")
    output = bytes(reader.output)
    if process.returncode != 0:
        detail = output.decode("utf-8", errors="replace").strip()
        raise MeasurementError(f"Git subject inspection failed: {detail or 'unknown Git error'}")
    return output


def clean_subject(subject_argument: Path) -> tuple[Path, str]:
    try:
        subject_exists = subject_argument.exists()
        subject_is_directory = subject_argument.is_dir()
    except OSError as error:
        raise MeasurementError(
            f"The measurement subject path could not be inspected: {error}"
        ) from error
    if not subject_exists or not subject_is_directory:
        raise MeasurementError("The measurement subject must be an existing repository directory.")
    subject = resolve_path(subject_argument, "measurement subject")
    reported_root = run_git(subject, ["rev-parse", "--show-toplevel"])
    repository_root = resolve_path(
        Path(reported_root.decode("utf-8").strip()), "Git-reported repository root"
    )
    if repository_root != subject:
        raise MeasurementError("The measurement subject must be the repository root, not a child path.")

    status = run_git(subject, CLEAN_STATUS_COMMAND.split()[1:])
    if status:
        raise MeasurementError("The measurement subject must be clean before planning.")

    revision = run_git(subject, ["rev-parse", "--verify", "HEAD"]).decode("utf-8").strip()
    if len(revision) != 40 or any(character not in "0123456789abcdef" for character in revision):
        raise MeasurementError("The measurement subject did not report one full Git revision.")

    missing = []
    for relative in REQUIRED_RELATIVE_PATHS:
        candidate = subject / relative
        try:
            available = candidate.is_file() and not candidate.is_symlink()
        except OSError as error:
            raise MeasurementError(
                f"The required public path could not be inspected: {relative}: {error}"
            ) from error
        if not available:
            missing.append(relative)
    if missing:
        rendered = ", ".join(missing)
        raise MeasurementError(f"The measurement subject lacks required public paths: {rendered}")
    return subject, revision


def validate_output(subject: Path, output_argument: Path) -> Path:
    output = resolve_path(output_argument, "measurement output")
    if is_within(output, subject):
        raise MeasurementError("The measurement output must live outside the measurement subject.")
    try:
        output_exists = output.exists()
        output_parent_is_directory = output.parent.is_dir()
    except OSError as error:
        raise MeasurementError(
            f"The measurement output path could not be inspected: {error}"
        ) from error
    if output_exists:
        raise MeasurementError("The measurement output directory must not already exist.")
    if not output_parent_is_directory:
        raise MeasurementError("The measurement output parent directory must already exist.")
    return output


def workflow_metric_ids() -> set[str]:
    return {
        metric_id
        for workflow in (*STEPS, *BLOCKED_WORKFLOWS)
        for metric_id in workflow["metric_ids"]
    }


def load_metric_requirements(subject: Path) -> list[dict[str, Any]]:
    catalog_path = subject / METRIC_CATALOG_RELATIVE_PATH
    try:
        with catalog_path.open("rb") as catalog_file:
            encoded = catalog_file.read(MAX_METRIC_CATALOG_BYTES + 1)
    except OSError as error:
        raise MeasurementError(f"The metric catalog could not be read: {error}") from error
    if len(encoded) > MAX_METRIC_CATALOG_BYTES:
        raise MeasurementError("The metric catalog exceeded the bounded planning-input limit.")
    try:
        catalog = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError(f"The metric catalog is not valid bounded JSON: {error}") from error
    if not isinstance(catalog, dict) or not isinstance(catalog.get("metrics"), list):
        raise MeasurementError("The metric catalog lacks a metrics array.")

    requirements: dict[str, dict[str, Any]] = {}
    for metric in catalog["metrics"]:
        if not isinstance(metric, dict) or metric.get("collector") != METRIC_COLLECTOR:
            continue
        metric_id = metric.get("id")
        minimum_samples = metric.get("minimum_samples")
        metric_subject = metric.get("subject")
        workload_id = metric.get("workload_id")
        if (
            not isinstance(metric_id, str)
            or not metric_id
            or not isinstance(minimum_samples, int)
            or isinstance(minimum_samples, bool)
            or minimum_samples <= 0
            or not isinstance(metric_subject, str)
            or not metric_subject
            or not isinstance(workload_id, str)
            or not workload_id
        ):
            raise MeasurementError("The metric catalog contains an invalid U14 measurement requirement.")
        if metric_id in requirements:
            raise MeasurementError(f"The metric catalog repeats U14 metric `{metric_id}`.")
        requirements[metric_id] = {
            "id": metric_id,
            "minimum_samples": minimum_samples,
            "subject": metric_subject,
            "workload_id": workload_id,
        }

    planned_ids = workflow_metric_ids()
    catalog_ids = set(requirements)
    missing = sorted(planned_ids - catalog_ids)
    unexpected = sorted(catalog_ids - planned_ids)
    if missing or unexpected:
        details = []
        if missing:
            details.append(f"missing planned metrics: {', '.join(missing)}")
        if unexpected:
            details.append(f"unplanned catalog metrics: {', '.join(unexpected)}")
        raise MeasurementError(
            "The metric catalog and the prepared U14 workflow disagree: " + "; ".join(details)
        )
    return [requirements[metric_id] for metric_id in sorted(requirements)]


def plan_payload(revision: str, metric_requirements: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema": PLAN_SCHEMA,
        "format_version": 1,
        "status": "prepared_not_executed",
        "decision": "not_evaluated",
        "source": {
            "revision": revision,
            "required_paths": list(REQUIRED_RELATIVE_PATHS),
        },
        "metric_catalog": {
            "path": METRIC_CATALOG_RELATIVE_PATH,
            "collector": METRIC_COLLECTOR,
            "requirements": metric_requirements,
        },
        "raw_sample_fields": list(RAW_SAMPLE_FIELDS),
        "isolation": {
            "worktree": "create a detached Git worktree under the output directory only when collection is admitted",
            "target_directory": "place Cargo targets under that detached worktree",
            "home_directory": "do not infer or create a default home directory; later collection must configure an explicit isolated home under output",
            "source_mutation": "forbidden",
        },
        "environment_fields": list(ENVIRONMENT_FIELDS),
        "steps": list(STEPS),
        "blocked_workflows": list(BLOCKED_WORKFLOWS),
        "unavailable_measurements": list(UNAVAILABLE_MEASUREMENTS),
        "non_claims": [
            "This file is a prepared local collection plan, not a benchmark result.",
            "This helper does not evaluate protocol ranges or emit Continue, Redirect, or Stop.",
            "Desktop manual playability requires an explicit human observation and cannot be inferred from a process exit.",
            "Unavailable measurements require a concrete tracer decision before they can enter a baseline.",
        ],
    }


def write_plan(output: Path, payload: dict[str, Any]) -> None:
    try:
        output.mkdir()
        plan_path = output / PLAN_FILENAME
        temporary_path = output / f"{PLAN_FILENAME}.tmp"
        encoded = json.dumps(payload, indent=2, sort_keys=True) + "\n"
        temporary_path.write_text(encoded, encoding="utf-8", newline="\n")
        temporary_path.replace(plan_path)
    except OSError as error:
        raise MeasurementError(f"The measurement plan could not be written: {error}") from error


def create_plan(subject_argument: Path, output_argument: Path) -> None:
    subject, revision = clean_subject(subject_argument)
    output = validate_output(subject, output_argument)
    metric_requirements = load_metric_requirements(subject)
    write_plan(output, plan_payload(revision, metric_requirements))


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Prepare one isolated reference-game first-playable measurement plan; "
            "this helper does not evaluate a performance or product result."
        )
    )
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser(
        "plan",
        help="validate a clean subject and write a non-executing collection plan",
    )
    plan.add_argument("--subject", type=Path, required=True, help="clean repository root to inspect")
    plan.add_argument("--output", type=Path, required=True, help="new directory outside the subject")
    historical = commands.add_parser(
        "verify-historical",
        help="verify one preserved historical raw-sample archive without upgrading its status",
    )
    historical.add_argument("--archive", type=Path, required=True)
    return parser


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "plan":
            create_plan(options.subject, options.output)
            return 0
        if options.command == "verify-historical":
            verify_historical_archive(options.archive)
            return 0
    except MeasurementError as error:
        print(f"measurement-helper: {error}", file=sys.stderr)
        return 2
    raise AssertionError(f"unhandled command: {options.command}")


if __name__ == "__main__":
    raise SystemExit(main())
