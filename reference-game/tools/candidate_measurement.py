#!/usr/bin/env python3
"""Collect bounded raw candidate-process observations without evaluating evidence."""

from __future__ import annotations

from dataclasses import dataclass, replace
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
from typing import BinaryIO, Callable, Sequence

import smoke_artifact


RAW_OBSERVATION_SCHEMA = "nara.reference-game.candidate-process-observation-v1"
RAW_OBSERVATION_FILENAME = "observation.json"
STARTUP_MARKER_ENV = "NARA_REFERENCE_GAME_STARTUP_MARKER"
STARTUP_MARKER_SCHEMA = "nara-reference-game.startup-marker-v1"
START_BOUNDARY_ID = "child_process_spawn_requested_v1"
FINAL_END_BOUNDARY_ID = "slower_of_headless_first_tick_and_desktop_first_present_v1"
METHOD_ID = "paired_monotonic_max_v1"
METRIC_ID = "candidate.startup_p95_ns"
ENVIRONMENT_CLASS = "desktop_timing_v1"
POPULATION = "warm"
MAX_COMBINED_OUTPUT_BYTES = 64 * 1024
MAX_OBSERVATION_BYTES = 64 * 1024
PROCESS_TIMEOUT_SECONDS = 60.0
PROCESS_POLL_SECONDS = 0.01
READER_JOIN_SECONDS = 5.0
MAX_U64 = (1 << 64) - 1


class CandidateMeasurementError(RuntimeError):
    """A static collector-input or process-control refusal."""


@dataclass(frozen=True)
class ProcessObservation:
    started_at_unix_ns: int
    completed_at_unix_ns: int
    exit_code: int | None
    failure: str | None
    marker_count: int
    marker_duration_ns: int | None
    stdout: bytes
    stderr: bytes

    @property
    def succeeded(self) -> bool:
        return self.failure is None


@dataclass(frozen=True)
class CandidateProcessSpec:
    collector_id: str
    layout_key: str
    entry_point: str
    marker_event: str
    component_boundary_id: str
    command_prefix: tuple[str, ...]
    command_arguments: tuple[str, ...]
    validate_stdout: Callable[[bytes, bytes], bool]


@dataclass(frozen=True)
class PreparedCandidate:
    validated: smoke_artifact.ValidatedArchive
    package_root: Path
    cwd: Path
    environment: dict[str, str]


class BoundedCapture:
    def __init__(self, started_at_monotonic_ns: int, marker_line: bytes) -> None:
        self._started_at_monotonic_ns = started_at_monotonic_ns
        self._marker_line = marker_line
        self._lock = threading.Lock()
        self._total_bytes = 0
        self.marker_count = 0
        self.marker_duration_ns: int | None = None
        self.overflowed = threading.Event()
        self.read_failed = threading.Event()

    def append(self, destination: bytearray, chunk: bytes) -> bool:
        with self._lock:
            remaining = MAX_COMBINED_OUTPUT_BYTES - self._total_bytes
            admitted = min(len(chunk), max(remaining, 0))
            destination.extend(chunk[:admitted])
            self._total_bytes += admitted
            if admitted != len(chunk):
                self.overflowed.set()
                return False
        return True

    def observe_stdout_lines(self, pending: bytearray, chunk: bytes) -> None:
        pending.extend(chunk)
        while True:
            newline = pending.find(b"\n")
            if newline < 0:
                return
            line = bytes(pending[: newline + 1])
            del pending[: newline + 1]
            if line != self._marker_line:
                continue
            observed_at = time.monotonic_ns()
            with self._lock:
                self.marker_count += 1
                if self.marker_duration_ns is None:
                    self.marker_duration_ns = max(
                        observed_at - self._started_at_monotonic_ns, 0
                    )


def startup_marker_line(event: str) -> bytes:
    if event not in {
        "headless_first_authoritative_tick",
        "desktop_first_playable_present",
    }:
        raise CandidateMeasurementError("startup marker event is unsupported")
    return (
        f'{{"schema":"{STARTUP_MARKER_SCHEMA}","event":"{event}"}}\n'
    ).encode("ascii")


def drain_process_output(
    stream: BinaryIO,
    destination: bytearray,
    capture: BoundedCapture,
    inspect_markers: bool,
) -> None:
    pending = bytearray()
    try:
        while chunk := stream.read(8 * 1024):
            if not capture.append(destination, chunk):
                return
            if inspect_markers:
                capture.observe_stdout_lines(pending, chunk)
    except (OSError, ValueError):
        capture.read_failed.set()
    finally:
        try:
            stream.close()
        except OSError:
            pass


def stopped_observation(
    started_at_unix_ns: int,
    failure: str,
    stdout: bytes = b"",
    stderr: bytes = b"",
) -> ProcessObservation:
    return ProcessObservation(
        started_at_unix_ns=started_at_unix_ns,
        completed_at_unix_ns=time.time_ns(),
        exit_code=None,
        failure=failure,
        marker_count=0,
        marker_duration_ns=None,
        stdout=stdout,
        stderr=stderr,
    )


def observe_process(
    command: Sequence[str],
    cwd: Path,
    environment: dict[str, str],
    marker_event: str,
    timeout_seconds: float = PROCESS_TIMEOUT_SECONDS,
) -> ProcessObservation:
    if not command or timeout_seconds <= 0:
        raise CandidateMeasurementError("candidate process configuration is invalid")
    marker_line = startup_marker_line(marker_event)
    started_at_unix_ns = time.time_ns()
    started_at_monotonic_ns = time.monotonic_ns()
    try:
        process = subprocess.Popen(
            list(command),
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=os.name == "posix",
        )
    except OSError:
        return stopped_observation(started_at_unix_ns, "process_start_failed")
    if process.stdout is None or process.stderr is None:
        try:
            smoke_artifact.stop_bounded_process(process)
        except smoke_artifact.ArtifactError as error:
            raise CandidateMeasurementError(
                "candidate process could not be stopped"
            ) from error
        return stopped_observation(started_at_unix_ns, "process_pipe_unavailable")

    capture = BoundedCapture(started_at_monotonic_ns, marker_line)
    stdout = bytearray()
    stderr = bytearray()
    readers = (
        threading.Thread(
            target=drain_process_output,
            args=(process.stdout, stdout, capture, True),
            daemon=True,
        ),
        threading.Thread(
            target=drain_process_output,
            args=(process.stderr, stderr, capture, False),
            daemon=True,
        ),
    )
    for reader in readers:
        reader.start()

    deadline = time.monotonic() + timeout_seconds
    failure: str | None = None
    while process.poll() is None:
        if capture.overflowed.is_set():
            failure = "process_output_limit"
            break
        if capture.read_failed.is_set():
            failure = "process_output_read_failed"
            break
        if time.monotonic() >= deadline:
            failure = "process_timeout"
            break
        time.sleep(PROCESS_POLL_SECONDS)

    if failure is not None and process.poll() is None:
        try:
            smoke_artifact.stop_bounded_process(process)
        except smoke_artifact.ArtifactError as error:
            raise CandidateMeasurementError(
                "candidate process could not be stopped"
            ) from error
    for reader in readers:
        reader.join(timeout=READER_JOIN_SECONDS)
    if any(reader.is_alive() for reader in readers):
        try:
            smoke_artifact.stop_bounded_process(process)
        except smoke_artifact.ArtifactError as error:
            raise CandidateMeasurementError(
                "candidate process could not be stopped"
            ) from error
        for reader in readers:
            reader.join(timeout=1.0)
        if any(reader.is_alive() for reader in readers):
            raise CandidateMeasurementError(
                "candidate output readers did not terminate"
            )

    if failure is None and capture.overflowed.is_set():
        failure = "process_output_limit"
    if failure is None and capture.read_failed.is_set():
        failure = "process_output_read_failed"
    if failure is None and process.returncode != 0:
        failure = "process_exit_failed"
    if failure is None and stderr:
        failure = "process_stderr_nonempty"
    if failure is None and capture.marker_count == 0:
        failure = "startup_marker_missing"
    if failure is None and capture.marker_count != 1:
        failure = "startup_marker_duplicate"

    return ProcessObservation(
        started_at_unix_ns=started_at_unix_ns,
        completed_at_unix_ns=time.time_ns(),
        exit_code=process.returncode,
        failure=failure,
        marker_count=capture.marker_count,
        marker_duration_ns=capture.marker_duration_ns,
        stdout=bytes(stdout),
        stderr=bytes(stderr),
    )


def validate_headless_stdout(stdout: bytes, marker_line: bytes) -> bool:
    lines = stdout.splitlines(keepends=True)
    if len(lines) != 2 or lines[0] != marker_line:
        return False
    try:
        summary = json.loads(lines[1])
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError):
        return False
    return (
        isinstance(summary, dict)
        and set(summary)
        == {
            "schema",
            "outcome",
            "tick",
            "score",
            "player_hit_points",
            "enemies_remaining",
            "projectiles_remaining",
        }
        and summary["schema"] == smoke_artifact.HEADLESS_SUMMARY_SCHEMA
        and summary["outcome"] in {"completed", "defeated"}
        and all(
            isinstance(summary[key], int) and not isinstance(summary[key], bool)
            for key in (
                "tick",
                "score",
                "player_hit_points",
                "enemies_remaining",
                "projectiles_remaining",
            )
        )
    )


def validate_desktop_stdout(stdout: bytes, marker_line: bytes) -> bool:
    return stdout == marker_line + smoke_artifact.DESKTOP_PROBE_SUCCESS


def with_output_validation(
    observation: ProcessObservation,
    marker_event: str,
    validator: Callable[[bytes, bytes], bool],
) -> ProcessObservation:
    if observation.succeeded and not validator(
        observation.stdout, startup_marker_line(marker_event)
    ):
        return replace(observation, failure="process_output_invalid")
    return observation


def skipped_after_warmup(warmup: ProcessObservation) -> ProcessObservation:
    now = time.time_ns()
    return ProcessObservation(
        started_at_unix_ns=now,
        completed_at_unix_ns=now,
        exit_code=None,
        failure="measurement_skipped_after_warmup_failure",
        marker_count=0,
        marker_duration_ns=None,
        stdout=b"",
        stderr=b"",
    )


def validate_sample_index(sample_index: int) -> int:
    if isinstance(sample_index, bool) or sample_index <= 0 or sample_index > MAX_U64:
        raise CandidateMeasurementError("sample index is invalid")
    return sample_index


def validate_work_root(work_root: Path) -> Path:
    try:
        smoke_artifact.package.require_directory(work_root, "measurement work root")
        resolved = work_root.resolve(strict=True)
        if next(resolved.iterdir(), None) is not None:
            raise CandidateMeasurementError("measurement work root must be empty")
        return resolved
    except smoke_artifact.package.PackageError as error:
        raise CandidateMeasurementError("measurement work root is invalid") from error
    except OSError as error:
        raise CandidateMeasurementError("measurement work root is invalid") from error


def validate_output_destination(output: Path, work_root: Path) -> Path:
    try:
        if os.path.lexists(output):
            raise CandidateMeasurementError("measurement output already exists")
        smoke_artifact.package.require_directory(output.parent, "measurement output parent")
        parent = output.parent.resolve(strict=True)
        destination = (parent / output.name).resolve(strict=False)
        if not output.name or destination.parent != parent:
            raise CandidateMeasurementError("measurement output path is invalid")
        if smoke_artifact.package.path_is_within(
            destination, work_root
        ) or smoke_artifact.package.path_is_within(work_root, destination):
            raise CandidateMeasurementError(
                "measurement output and work root must be disjoint"
            )
        return destination
    except smoke_artifact.package.PackageError as error:
        raise CandidateMeasurementError("measurement output path is invalid") from error
    except OSError as error:
        raise CandidateMeasurementError("measurement output path is invalid") from error


def prepare_candidate(
    archive: Path,
    work_root: Path,
    expected_platform: str,
    expected_source_revision: str,
    environment_overrides: dict[str, str],
) -> PreparedCandidate:
    if STARTUP_MARKER_ENV in environment_overrides:
        raise CandidateMeasurementError("startup marker environment is collector-owned")
    try:
        validated = smoke_artifact.extract_archive(
            archive,
            work_root / "consumer",
            expected_platform,
            expected_source_revision,
        )
        environment, cwd = smoke_artifact.smoke_environment(
            work_root, environment_overrides
        )
    except (OSError, smoke_artifact.ArtifactError) as error:
        raise CandidateMeasurementError("candidate preparation failed") from error
    environment[STARTUP_MARKER_ENV] = "1"
    return PreparedCandidate(
        validated=validated,
        package_root=work_root / "consumer" / validated.layout.package_root,
        cwd=cwd,
        environment=environment,
    )


def regular_file_descriptor(path: str, contents: bytes) -> dict[str, object]:
    return {
        "path": path,
        "bytes": len(contents),
        "sha256": hashlib.sha256(contents).hexdigest(),
    }


def process_record(
    observation: ProcessObservation,
    stdout_path: str,
    stderr_path: str,
) -> dict[str, object]:
    return {
        "completed_at_unix_ns": observation.completed_at_unix_ns,
        "exit_code": observation.exit_code,
        "failure": observation.failure,
        "marker_count": observation.marker_count,
        "marker_duration_ns": observation.marker_duration_ns,
        "started_at_unix_ns": observation.started_at_unix_ns,
        "stderr": regular_file_descriptor(stderr_path, observation.stderr),
        "stdout": regular_file_descriptor(stdout_path, observation.stdout),
    }


def raw_observation_payload(
    prepared: PreparedCandidate,
    spec: CandidateProcessSpec,
    sample_index: int,
    warmup: ProcessObservation,
    sample: ProcessObservation,
) -> dict[str, object]:
    status = "observed" if warmup.succeeded and sample.succeeded else "failed"
    return {
        "schema": RAW_OBSERVATION_SCHEMA,
        "format_version": 1,
        "status": status,
        "decision": "not_evaluated",
        "collector": {
            "id": spec.collector_id,
            "version": 1,
            "trust": "credential_free_untrusted",
        },
        "candidate": smoke_artifact.receipt(prepared.validated),
        "measurement": {
            "component_boundary_id": spec.component_boundary_id,
            "entry_point": spec.entry_point,
            "environment_class": ENVIRONMENT_CLASS,
            "final_end_boundary_id": FINAL_END_BOUNDARY_ID,
            "marker_event": spec.marker_event,
            "method_id": METHOD_ID,
            "metric_id": METRIC_ID,
            "population": POPULATION,
            "sample_index": sample_index,
            "start_boundary_id": START_BOUNDARY_ID,
            "value_ns": sample.marker_duration_ns if sample.succeeded else None,
        },
        "warmup": process_record(
            warmup,
            "raw/warmup-stdout.bin",
            "raw/warmup-stderr.bin",
        ),
        "sample": process_record(
            sample,
            "raw/sample-stdout.bin",
            "raw/sample-stderr.bin",
        ),
        "non_claims": [
            "This is one untrusted raw component observation, not a U22 evidence envelope.",
            "The collector does not pair components, aggregate a percentile, or make a product decision.",
            "Restricted process output remains outside repository evidence and is represented only by bounded digest records.",
        ],
    }


def write_new_file(path: Path, contents: bytes) -> None:
    try:
        with path.open("xb") as destination:
            destination.write(contents)
            destination.flush()
            os.fsync(destination.fileno())
    except OSError as error:
        raise CandidateMeasurementError("measurement output could not be written") from error


def safe_remove_temporary(path: Path, parent: Path) -> None:
    try:
        resolved_parent = parent.resolve(strict=True)
        resolved_path = path.resolve(strict=True)
    except OSError:
        return
    if (
        smoke_artifact.package.path_is_within(resolved_path, resolved_parent)
        and resolved_path != resolved_parent
        and resolved_path.name.startswith(".nara-measurement-")
    ):
        shutil.rmtree(resolved_path, ignore_errors=True)


def publish_raw_observation(
    output: Path,
    payload: dict[str, object],
    warmup: ProcessObservation,
    sample: ProcessObservation,
) -> None:
    encoded = (
        json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=True) + "\n"
    ).encode("utf-8")
    if len(encoded) > MAX_OBSERVATION_BYTES:
        raise CandidateMeasurementError("measurement observation exceeds its byte limit")
    parent = output.parent
    try:
        temporary = Path(
            tempfile.mkdtemp(prefix=".nara-measurement-", dir=parent)
        )
    except OSError as error:
        raise CandidateMeasurementError(
            "measurement output could not be prepared"
        ) from error
    try:
        raw = temporary / "raw"
        raw.mkdir()
        write_new_file(raw / "warmup-stdout.bin", warmup.stdout)
        write_new_file(raw / "warmup-stderr.bin", warmup.stderr)
        write_new_file(raw / "sample-stdout.bin", sample.stdout)
        write_new_file(raw / "sample-stderr.bin", sample.stderr)
        write_new_file(temporary / RAW_OBSERVATION_FILENAME, encoded)
        if os.path.lexists(output):
            raise CandidateMeasurementError("measurement output already exists")
        temporary.rename(output)
    except CandidateMeasurementError:
        safe_remove_temporary(temporary, parent)
        raise
    except OSError as error:
        safe_remove_temporary(temporary, parent)
        raise CandidateMeasurementError(
            "measurement output could not be published"
        ) from error


def collect_candidate_startup(
    archive: Path,
    work_root_argument: Path,
    output_argument: Path,
    expected_platform: str,
    expected_source_revision: str,
    sample_index: int,
    spec: CandidateProcessSpec,
    environment_overrides: dict[str, str],
) -> bool:
    sample_index = validate_sample_index(sample_index)
    work_root = validate_work_root(work_root_argument)
    output = validate_output_destination(output_argument, work_root)
    prepared = prepare_candidate(
        archive,
        work_root,
        expected_platform,
        expected_source_revision,
        environment_overrides,
    )
    executable = prepared.package_root / prepared.validated.manifest["layout"][spec.layout_key]
    command = [
        *spec.command_prefix,
        str(executable),
        *spec.command_arguments,
    ]
    warmup = with_output_validation(
        observe_process(
            command,
            prepared.cwd,
            prepared.environment,
            spec.marker_event,
        ),
        spec.marker_event,
        spec.validate_stdout,
    )
    sample = (
        with_output_validation(
            observe_process(
                command,
                prepared.cwd,
                prepared.environment,
                spec.marker_event,
            ),
            spec.marker_event,
            spec.validate_stdout,
        )
        if warmup.succeeded
        else skipped_after_warmup(warmup)
    )
    payload = raw_observation_payload(
        prepared, spec, sample_index, warmup, sample
    )
    publish_raw_observation(output, payload, warmup, sample)
    return warmup.succeeded and sample.succeeded
