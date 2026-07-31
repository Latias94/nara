#!/usr/bin/env python3
"""Collect and verify one isolated, non-decisive first-playable baseline."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
from datetime import UTC, datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import shutil
import subprocess
import sys
import threading
import time
from typing import Any, Sequence


PLAN_SCHEMA = "nara.reference-game.first-playable-plan-v1"
RUN_SCHEMA = "nara.reference-game.first-playable-local-run-v2"
PLAN_FILENAME = "measurement-plan.json"
RUN_FILENAME = "run-manifest.json"
RAW_FILENAME = "raw-samples.jsonl"
CATALOG_PATH = "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json"
COLLECTOR_ID = "u14"

MAX_GIT_OUTPUT = 64 * 1024
MAX_COMMAND_OUTPUT = 1024 * 1024
MAX_CATALOG_BYTES = 256 * 1024
MAX_MANIFEST_BYTES = 256 * 1024
MAX_RAW_BYTES = 4 * 1024 * 1024
MAX_RAW_LINE_BYTES = 64 * 1024
MAX_RAW_SAMPLES = 4096
MAX_SOURCE_BYTES = 2 * 1024 * 1024
PROCESS_JOIN_SECONDS = 1.0
GIT_TIMEOUT_SECONDS = 10.0
DEFAULT_COMMAND_TIMEOUT_SECONDS = 30 * 60
CLEAN_STATUS_ARGUMENTS = ("status", "--porcelain=v1", "-z")

REQUIRED_PATHS = (
    "Cargo.toml",
    "reference-game/Cargo.toml",
    "reference-game/Cargo.lock",
    "reference-game/src/bin/headless.rs",
    "reference-game/src/bin/desktop.rs",
    "reference-game/src/bin/desktop_render_probe.rs",
    "reference-game/src/systems.rs",
    "reference-game/scenes/startup.scene.json",
    "reference-game/tests/public_surface.rs",
    CATALOG_PATH,
)

AUTOMATIC_METRICS = frozenset(
    {
        "build.cold_ns",
        "build.incremental_ns",
        "gameplay.headless_wave_success",
        "iteration.body.p50_ns",
        "iteration.body.p95_ns",
        "iteration.data.p50_ns",
        "iteration.data.p95_ns",
        "iteration.structural.p50_ns",
        "iteration.structural.p95_ns",
        "journey.clean_to_headless_wave_ns",
    }
)
MANUAL_METRICS = frozenset(
    {
        "gameplay.desktop_playable_success",
        "journey.clean_to_desktop_playable_ns",
    }
)
UNAVAILABLE_REASONS = {
    "frame.p99_ns": "The public desktop product emits no bounded frame-time sample stream.",
    "module.add.success": "No committed public module-addition task exists.",
    "module.add.time_ns": "No committed public module-addition task exists.",
    "public.production.coverage_basis_points": (
        "A passing static public-surface test is not an executed production-call denominator."
    ),
    "runtime.gpu_resource_bytes": (
        "Backend cache statistics are not exposed by the public desktop product entry point."
    ),
    "runtime.memory_bytes": (
        "No cross-platform public process-memory collector is selected for this product path."
    ),
    "slot.configure.success": "No committed public window-slot task exists.",
    "slot.configure.time_ns": "No committed public window-slot task exists.",
}
CATALOG_METRICS = AUTOMATIC_METRICS | MANUAL_METRICS | frozenset(UNAVAILABLE_REASONS)
TWIN_METRICS = (
    ("iteration.body.p50_ns", "iteration.body.p95_ns"),
    ("iteration.data.p50_ns", "iteration.data.p95_ns"),
    ("iteration.structural.p50_ns", "iteration.structural.p95_ns"),
)
SEMANTIC_METRICS = frozenset(
    {
        "gameplay.headless_wave_success",
        "journey.clean_to_headless_wave_ns",
        "iteration.body.p50_ns",
        "iteration.body.p95_ns",
        "iteration.data.p50_ns",
        "iteration.data.p95_ns",
        "iteration.structural.p50_ns",
        "iteration.structural.p95_ns",
    }
)
EXTRA_UNAVAILABLE = (
    {
        "id": "render.packet.batch_count",
        "reason": "The public desktop path exports no render-packet batch counter.",
    },
    {
        "id": "render.packet.instance_count",
        "reason": "The public desktop path exports no render-packet instance counter.",
    },
    {
        "id": "render.packet.retained_bytes",
        "reason": "The current packet does not publish retained payload bytes.",
    },
    {
        "id": "render.packet.clone_bytes",
        "reason": "The current renderer exports no packet clone or allocation counter.",
    },
)

BUILD_ARGS = ("build", "--locked", "--bins", "--jobs", "1")
HEADLESS_ARGS = ("run", "--locked", "--bin", "headless", "--", "--max-ticks", "96")
PUBLIC_SURFACE_ARGS = ("test", "--locked", "--test", "public_surface", "--jobs", "1")

STEPS = (
    {
        "id": "build_timings",
        "kind": "automatic",
        "command": ["cargo", *BUILD_ARGS],
        "metric_ids": ["build.cold_ns", "build.incremental_ns"],
    },
    {
        "id": "clean_headless_wave",
        "kind": "automatic",
        "command": ["cargo", *HEADLESS_ARGS],
        "metric_ids": [
            "journey.clean_to_headless_wave_ns",
            "gameplay.headless_wave_success",
        ],
    },
    {
        "id": "body_edit_reload",
        "kind": "automatic",
        "mutation": "toggle one reviewed behavior-neutral Rust body edit",
        "metric_ids": ["iteration.body.p50_ns", "iteration.body.p95_ns"],
    },
    {
        "id": "data_edit_reload",
        "kind": "automatic",
        "mutation": "toggle Player hit-points through parsed scene JSON",
        "metric_ids": ["iteration.data.p50_ns", "iteration.data.p95_ns"],
    },
    {
        "id": "structural_rust_edit",
        "kind": "automatic",
        "mutation": "toggle one private behavior-neutral Rust field",
        "metric_ids": [
            "iteration.structural.p50_ns",
            "iteration.structural.p95_ns",
        ],
    },
    {
        "id": "desktop_manual_playthrough",
        "kind": "manual",
        "command": [
            "cargo",
            "run",
            "--locked",
            "--features",
            "desktop",
            "--bin",
            "desktop",
        ],
        "metric_ids": sorted(MANUAL_METRICS),
    },
    {
        "id": "public_production_coverage",
        "kind": "check_only",
        "command": ["cargo", *PUBLIC_SURFACE_ARGS],
        "metric_ids": ["public.production.coverage_basis_points"],
    },
)
BLOCKED_WORKFLOWS = (
    {
        "id": "module_addition",
        "metric_ids": ["module.add.time_ns", "module.add.success"],
        "reason": UNAVAILABLE_REASONS["module.add.time_ns"],
    },
    {
        "id": "window_slot_configuration",
        "metric_ids": ["slot.configure.time_ns", "slot.configure.success"],
        "reason": UNAVAILABLE_REASONS["slot.configure.time_ns"],
    },
    {
        "id": "desktop_pressure_telemetry",
        "metric_ids": [
            "frame.p99_ns",
            "runtime.gpu_resource_bytes",
            "runtime.memory_bytes",
        ],
        "reason": "The public desktop product lacks the required bounded telemetry.",
    },
)

RAW_FIELDS = (
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
    "command_output_reference",
    "result_digest",
)
RAW_FIELD_SET = frozenset(RAW_FIELDS)
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
REQUIREMENT_FIELDS = (
    "id",
    "minimum_samples",
    "subject",
    "workload_id",
    "value_kind",
    "population",
    "start_boundary_id",
    "end_boundary_id",
    "method_id",
    "environment_class",
)
SCRATCH_DIRECTORIES = ("target", "cargo-home", "home", "temp")
RUN_NON_CLAIMS = (
    "This local run does not grant release or publication authority.",
    "Missing manual or telemetry metrics remain missing; process exit is not substituted.",
    "The verifier checks integrity and shape, not performance targets or product verdicts.",
)

BODY_BASE = b"""pub(crate) fn tick_project_weapons(mut weapons: Query<&mut Weapon, With<SceneEntitySource>>) {
    for mut weapon in &mut weapons {
        weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
    }
}
"""
BODY_VARIANT = BODY_BASE.replace(b"saturating_sub(1)", b"saturating_sub(1_u64)")
STRUCTURAL_BASE = (
    b"""struct MovementIntentCommand {
    direction: MovementDirection,
    pressed: bool,
}
""",
    b"""Ok(MovementIntentCommand {
        direction,
        pressed: *pressed,
    })
""",
)
STRUCTURAL_VARIANT = (
    b"""struct MovementIntentCommand {
    direction: MovementDirection,
    pressed: bool,
    _measurement_generation: u8,
}
""",
    b"""Ok(MovementIntentCommand {
        direction,
        pressed: *pressed,
        _measurement_generation: 0,
    })
""",
)


class MeasurementError(RuntimeError):
    """A stable refusal from the local measurement helper."""


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    output: bytes
    duration_ns: int
    completed_monotonic_ns: int
    started_at_utc: str
    completed_at_utc: str
    timed_out: bool = False
    overflowed: bool = False

    @property
    def failed(self) -> bool:
        return self.returncode != 0 or self.timed_out or self.overflowed


class OutputReader(threading.Thread):
    def __init__(self, stream: Any, limit: int) -> None:
        super().__init__(daemon=True)
        self.stream = stream
        self.limit = limit
        self.output = bytearray()
        self.overflowed = False
        self.error: Exception | None = None
        self.finished = threading.Event()

    def run(self) -> None:
        try:
            while chunk := self.stream.read1(8 * 1024):
                remaining = self.limit - len(self.output)
                if len(chunk) > remaining:
                    self.output.extend(chunk[: max(remaining, 0)])
                    self.overflowed = True
                    return
                self.output.extend(chunk)
        except (OSError, ValueError) as error:
            self.error = error
        finally:
            self.finished.set()


class WindowsJob:
    """Owns one Windows subprocess tree from its first instruction."""

    def __init__(self) -> None:
        import ctypes
        from ctypes import wintypes

        class BasicLimitInformation(ctypes.Structure):
            _fields_ = [
                ("per_process_user_time_limit", ctypes.c_longlong),
                ("per_job_user_time_limit", ctypes.c_longlong),
                ("limit_flags", wintypes.DWORD),
                ("minimum_working_set_size", ctypes.c_size_t),
                ("maximum_working_set_size", ctypes.c_size_t),
                ("active_process_limit", wintypes.DWORD),
                ("affinity", ctypes.c_size_t),
                ("priority_class", wintypes.DWORD),
                ("scheduling_class", wintypes.DWORD),
            ]

        class IoCounters(ctypes.Structure):
            _fields_ = [
                ("read_operation_count", ctypes.c_ulonglong),
                ("write_operation_count", ctypes.c_ulonglong),
                ("other_operation_count", ctypes.c_ulonglong),
                ("read_transfer_count", ctypes.c_ulonglong),
                ("write_transfer_count", ctypes.c_ulonglong),
                ("other_transfer_count", ctypes.c_ulonglong),
            ]

        class ExtendedLimitInformation(ctypes.Structure):
            _fields_ = [
                ("basic_limit_information", BasicLimitInformation),
                ("io_info", IoCounters),
                ("process_memory_limit", ctypes.c_size_t),
                ("job_memory_limit", ctypes.c_size_t),
                ("peak_process_memory_used", ctypes.c_size_t),
                ("peak_job_memory_used", ctypes.c_size_t),
            ]

        self.ctypes = ctypes
        self.wintypes = wintypes
        self.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        self.kernel32.CloseHandle.restype = wintypes.BOOL
        self.kernel32.CreateJobObjectW.restype = wintypes.HANDLE
        self.kernel32.SetInformationJobObject.argtypes = (
            wintypes.HANDLE,
            ctypes.c_int,
            ctypes.c_void_p,
            wintypes.DWORD,
        )
        self.kernel32.SetInformationJobObject.restype = wintypes.BOOL
        self.handle = self.kernel32.CreateJobObjectW(None, None)
        if not self.handle:
            raise self._error("A Windows process job could not be created")
        limits = ExtendedLimitInformation()
        limits.basic_limit_information.limit_flags = 0x00002000
        if not self.kernel32.SetInformationJobObject(
            self.handle,
            9,
            ctypes.byref(limits),
            ctypes.sizeof(limits),
        ):
            error = self._error("A Windows process job could not be bounded")
            self.close()
            raise error

    def _error(self, message: str) -> MeasurementError:
        return MeasurementError(f"{message} (Win32 error {self.ctypes.get_last_error()}).")

    def attach_and_resume(self, process_id: int) -> None:
        ctypes = self.ctypes
        wintypes = self.wintypes
        kernel32 = self.kernel32
        kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        process_handle = kernel32.OpenProcess(0x0001 | 0x0100 | 0x0800, False, process_id)
        if not process_handle:
            raise self._error("A suspended Windows process could not be opened")
        try:
            kernel32.AssignProcessToJobObject.argtypes = (wintypes.HANDLE, wintypes.HANDLE)
            kernel32.AssignProcessToJobObject.restype = wintypes.BOOL
            if not kernel32.AssignProcessToJobObject(self.handle, process_handle):
                raise self._error("A suspended Windows process could not join its job")
            ntdll = ctypes.WinDLL("ntdll")
            ntdll.NtResumeProcess.argtypes = (wintypes.HANDLE,)
            ntdll.NtResumeProcess.restype = wintypes.LONG
            status = ntdll.NtResumeProcess(process_handle)
            if status != 0:
                raise MeasurementError(
                    f"A suspended Windows process could not resume (NTSTATUS {status:#x})."
                )
        finally:
            kernel32.CloseHandle(process_handle)

    def terminate(self) -> None:
        self.kernel32.TerminateJobObject.argtypes = (self.wintypes.HANDLE, self.wintypes.UINT)
        self.kernel32.TerminateJobObject.restype = self.wintypes.BOOL
        if self.handle is not None and not self.kernel32.TerminateJobObject(self.handle, 1):
            raise self._error("A Windows process job could not be terminated")

    def close(self) -> None:
        if self.handle is not None:
            self.kernel32.CloseHandle(self.handle)
            self.handle = None


def utc_now() -> str:
    return datetime.now(UTC).isoformat(timespec="microseconds").replace("+00:00", "Z")


def digest(contents: bytes) -> str:
    return hashlib.sha256(contents).hexdigest()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def resolve(path: Path, purpose: str) -> Path:
    try:
        return path.resolve()
    except (OSError, RuntimeError) as error:
        raise MeasurementError(f"The {purpose} path could not be resolved: {error}") from error


def within(path: Path, parent: Path) -> bool:
    return path.is_relative_to(parent)


def read_bounded(path: Path, limit: int, purpose: str) -> bytes:
    try:
        with path.open("rb") as source:
            contents = source.read(limit + 1)
    except OSError as error:
        raise MeasurementError(f"The {purpose} could not be read: {error}") from error
    if len(contents) > limit:
        raise MeasurementError(f"The {purpose} exceeded its bounded byte limit.")
    return contents


def read_json(path: Path, limit: int, purpose: str) -> dict[str, Any]:
    try:
        value = json.loads(read_bounded(path, limit, purpose))
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError(f"The {purpose} is not valid bounded JSON.") from error
    if not isinstance(value, dict):
        raise MeasurementError(f"The {purpose} must be a JSON object.")
    return value


def write_bytes(path: Path, contents: bytes, purpose: str) -> None:
    try:
        path.write_bytes(contents)
    except OSError as error:
        raise MeasurementError(f"The {purpose} could not be written: {error}") from error


def write_json(path: Path, value: Any) -> None:
    temporary = path.with_name(f"{path.name}.tmp")
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    try:
        temporary.write_bytes(encoded)
        temporary.replace(path)
    except OSError as error:
        raise MeasurementError(f"The `{path.name}` artifact could not be written: {error}") from error


def stop_process(process: subprocess.Popen[bytes], windows_job: WindowsJob | None) -> None:
    group_error: OSError | MeasurementError | None = None
    if windows_job is not None:
        try:
            windows_job.terminate()
        except MeasurementError as error:
            group_error = error
    elif os.name != "nt":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except OSError as error:
            group_error = error
    if process.poll() is None:
        try:
            process.kill()
        except OSError as error:
            group_error = group_error or error
    try:
        process.wait(timeout=PROCESS_JOIN_SECONDS)
    except subprocess.TimeoutExpired as error:
        raise MeasurementError("A bounded process did not stop within its deadline.") from error
    if group_error is not None:
        raise MeasurementError(f"A bounded process tree could not be stopped: {group_error}")


def run_process(
    command: Sequence[str],
    cwd: Path,
    environment: dict[str, str],
    timeout_seconds: float,
    *,
    output_limit: int = MAX_COMMAND_OUTPUT,
    log_path: Path | None = None,
) -> CommandResult:
    started_at = utc_now()
    started = time.monotonic_ns()
    windows_job: WindowsJob | None = None
    process: subprocess.Popen[bytes] | None = None
    try:
        process_options: dict[str, Any] = {}
        if os.name == "nt":
            windows_job = WindowsJob()
            process_options["creationflags"] = (
                getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
                | getattr(subprocess, "CREATE_SUSPENDED", 0x00000004)
            )
        else:
            process_options["start_new_session"] = True
        process = subprocess.Popen(
            list(command),
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            **process_options,
        )
        if windows_job is not None:
            windows_job.attach_and_resume(process.pid)
    except BaseException as error:
        if process is not None:
            try:
                stop_process(process, windows_job)
            except MeasurementError:
                pass
        if windows_job is not None:
            windows_job.close()
        if isinstance(error, (OSError, MeasurementError)):
            raise MeasurementError(f"A bounded process could not start: {error}") from error
        raise
    try:
        if process.stdout is None:
            stop_process(process, windows_job)
            raise MeasurementError("A bounded process did not expose readable output.")
        reader = OutputReader(process.stdout, output_limit)
        reader.start()
        deadline = time.monotonic() + timeout_seconds
        timed_out = False
        if not reader.finished.wait(timeout_seconds):
            timed_out = True
            stop_process(process, windows_job)
        elif reader.error is not None or reader.overflowed:
            stop_process(process, windows_job)
        else:
            remaining = max(deadline - time.monotonic(), 0.0)
            try:
                process.wait(timeout=remaining)
            except subprocess.TimeoutExpired:
                timed_out = True
                stop_process(process, windows_job)
        reader.join(PROCESS_JOIN_SECONDS)
        if reader.is_alive():
            try:
                process.stdout.close()
            except OSError:
                pass
            reader.join(PROCESS_JOIN_SECONDS)
        if reader.is_alive() or (
            reader.error is not None and not timed_out and not reader.overflowed
        ):
            raise MeasurementError("A bounded process output reader did not finish cleanly.")
        # The command owns its whole process tree, including children that detached their output.
        stop_process(process, windows_job)
        completed_monotonic_ns = time.monotonic_ns()
        completed_at = utc_now()
        output = bytes(reader.output)
        if log_path is not None:
            write_bytes(log_path, output, "bounded command log")
        return CommandResult(
            returncode=process.returncode if process.returncode is not None else -1,
            output=output,
            duration_ns=completed_monotonic_ns - started,
            completed_monotonic_ns=completed_monotonic_ns,
            started_at_utc=started_at,
            completed_at_utc=completed_at,
            timed_out=timed_out,
            overflowed=reader.overflowed,
        )
    except BaseException:
        try:
            stop_process(process, windows_job)
        except MeasurementError:
            pass
        raise
    finally:
        if windows_job is not None:
            windows_job.close()


def git_environment(optional_locks: str) -> dict[str, str]:
    environment = {
        key: value for key, value in os.environ.items() if not key.upper().startswith("GIT_")
    }
    environment["GIT_OPTIONAL_LOCKS"] = optional_locks
    return environment


def run_git(subject: Path, arguments: Sequence[str]) -> bytes:
    result = run_process(
        ["git", "-C", os.fspath(subject), *arguments],
        subject,
        git_environment("0"),
        GIT_TIMEOUT_SECONDS,
        output_limit=MAX_GIT_OUTPUT,
    )
    if result.failed:
        detail = result.output.decode("utf-8", errors="replace").strip()
        raise MeasurementError(f"Git subject inspection failed: {detail or 'unknown Git error'}")
    return result.output


def clean_subject(argument: Path) -> tuple[Path, str]:
    try:
        if not argument.is_dir():
            raise MeasurementError("The measurement subject must be an existing directory.")
    except OSError as error:
        raise MeasurementError(f"The measurement subject could not be inspected: {error}") from error
    subject = resolve(argument, "measurement subject")
    reported = run_git(subject, ["rev-parse", "--show-toplevel"])
    if resolve(Path(reported.decode("utf-8").strip()), "Git repository root") != subject:
        raise MeasurementError("The measurement subject must be the repository root.")
    if run_git(subject, CLEAN_STATUS_ARGUMENTS):
        raise MeasurementError("The measurement subject must be clean.")
    revision = run_git(subject, ["rev-parse", "--verify", "HEAD"]).decode("utf-8").strip()
    if len(revision) != 40 or any(character not in "0123456789abcdef" for character in revision):
        raise MeasurementError("The measurement subject did not report one full revision.")
    missing = [
        relative
        for relative in REQUIRED_PATHS
        if not (subject / relative).is_file() or (subject / relative).is_symlink()
    ]
    if missing:
        raise MeasurementError(f"The measurement subject lacks required public paths: {', '.join(missing)}")
    return subject, revision


def new_output(subject: Path, argument: Path) -> Path:
    output = resolve(argument, "measurement output")
    if within(output, subject):
        raise MeasurementError("The measurement output must live outside the measurement subject.")
    if output.exists():
        raise MeasurementError("The measurement output directory must not already exist.")
    if not output.parent.is_dir():
        raise MeasurementError("The measurement output parent must be an existing directory.")
    return output


def load_requirements(subject: Path) -> tuple[list[dict[str, Any]], str]:
    catalog_path = subject / CATALOG_PATH
    catalog_bytes = read_bounded(catalog_path, MAX_CATALOG_BYTES, "metric catalog")
    try:
        catalog = json.loads(catalog_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError("The metric catalog is not valid JSON.") from error
    if not isinstance(catalog, dict):
        raise MeasurementError("The metric catalog root is not an object.")
    metrics = catalog.get("metrics")
    if not isinstance(metrics, list):
        raise MeasurementError("The metric catalog lacks its metrics array.")
    requirements: dict[str, dict[str, Any]] = {}
    for metric in metrics:
        if not isinstance(metric, dict) or metric.get("collector") != COLLECTOR_ID:
            continue
        metric_id = metric.get("id")
        minimum = metric.get("minimum_samples")
        if (
            not isinstance(metric_id, str)
            or not isinstance(minimum, int)
            or isinstance(minimum, bool)
            or minimum <= 0
            or minimum > MAX_RAW_SAMPLES
            or any(
                not isinstance(metric.get(field), str) or not metric[field]
                for field in REQUIREMENT_FIELDS
                if field not in {"id", "minimum_samples"}
            )
            or metric_id in requirements
        ):
            raise MeasurementError("The metric catalog contains an invalid U14 requirement.")
        requirements[metric_id] = {field: metric[field] for field in REQUIREMENT_FIELDS}
    if set(requirements) != CATALOG_METRICS:
        raise MeasurementError("The metric catalog and the prepared U14 workflow disagree.")
    for left, right in TWIN_METRICS:
        if requirements[left]["minimum_samples"] != requirements[right]["minimum_samples"]:
            raise MeasurementError("A P50/P95 metric pair has different sample floors.")
    if sum(requirements[metric]["minimum_samples"] for metric in AUTOMATIC_METRICS) > MAX_RAW_SAMPLES:
        raise MeasurementError("The automatic measurement population exceeds its sample budget.")
    return (
        [requirements[metric_id] for metric_id in sorted(requirements)],
        digest(catalog_bytes),
    )


def unavailable_measurements() -> list[dict[str, str]]:
    return [
        *(
            {"id": metric_id, "reason": reason}
            for metric_id, reason in sorted(UNAVAILABLE_REASONS.items())
        ),
        *EXTRA_UNAVAILABLE,
    ]


def plan_payload(
    revision: str,
    requirements: list[dict[str, Any]],
    catalog_sha256: str,
) -> dict[str, Any]:
    return {
        "schema": PLAN_SCHEMA,
        "format_version": 1,
        "status": "prepared_not_executed",
        "decision": "not_evaluated",
        "source": {"revision": revision, "required_paths": list(REQUIRED_PATHS)},
        "metric_catalog": {
            "path": CATALOG_PATH,
            "collector": COLLECTOR_ID,
            "sha256": catalog_sha256,
            "requirements": requirements,
        },
        "raw_sample_fields": list(RAW_FIELDS),
        "environment_fields": list(ENVIRONMENT_FIELDS),
        "steps": list(STEPS),
        "blocked_workflows": list(BLOCKED_WORKFLOWS),
        "unavailable_measurements": unavailable_measurements(),
        "isolation": {
            "worktree": "detached under the external output root",
            "home": "isolated under the external output root",
            "cargo": "one build job; offline after isolated fetch",
            "source_mutation": "forbidden",
        },
        "non_claims": [
            "This plan contains no measurements or product decision.",
            "Manual and unavailable metrics remain missing until directly observed.",
        ],
    }


def create_plan(subject_argument: Path, output_argument: Path) -> None:
    subject, revision = clean_subject(subject_argument)
    output = new_output(subject, output_argument)
    requirements, catalog_sha256 = load_requirements(subject)
    try:
        output.mkdir()
    except OSError as error:
        raise MeasurementError(f"The measurement output could not be created: {error}") from error
    write_json(
        output / PLAN_FILENAME,
        plan_payload(revision, requirements, catalog_sha256),
    )


def isolated_environment(output: Path, target: Path) -> dict[str, str]:
    allowed = {
        "ALL_PROXY",
        "COMSPEC",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "NO_PROXY",
        "PATH",
        "PATHEXT",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "SSL_CERT_DIR",
        "SSL_CERT_FILE",
        "SYSTEMROOT",
        "WINDIR",
    }
    environment = {
        key.upper(): value for key, value in os.environ.items() if key.upper() in allowed
    }
    if "RUSTUP_HOME" not in environment:
        for home_key in ("USERPROFILE", "HOME"):
            home = os.environ.get(home_key)
            candidate = Path(home) / ".rustup" if home else None
            if candidate is not None and candidate.is_dir():
                environment["RUSTUP_HOME"] = os.fspath(candidate)
                break
    home = output / "home"
    temporary = output / "temp"
    cargo_home = output / "cargo-home"
    app_data = home / "app-data"
    local_data = home / "local-data"
    for directory in (home, temporary, cargo_home, app_data, local_data):
        try:
            directory.mkdir(parents=True, exist_ok=True)
        except OSError as error:
            raise MeasurementError(f"The isolated environment could not be created: {error}") from error
    environment.update(
        {
            "APPDATA": os.fspath(app_data),
            "CARGO_BUILD_JOBS": "1",
            "CARGO_HOME": os.fspath(cargo_home),
            "CARGO_INCREMENTAL": "1",
            "CARGO_TARGET_DIR": os.fspath(target),
            "CARGO_TERM_COLOR": "never",
            "HOME": os.fspath(home),
            "LOCALAPPDATA": os.fspath(local_data),
            "TEMP": os.fspath(temporary),
            "TMP": os.fspath(temporary),
            "TMPDIR": os.fspath(temporary),
            "USERPROFILE": os.fspath(home),
        }
    )
    return environment


def native_anchor(contents: bytes, anchor: bytes) -> bytes:
    return anchor.replace(b"\n", b"\r\n") if b"\r\n" in contents else anchor


def rust_variant(contents: bytes, base: Sequence[bytes], variant: Sequence[bytes]) -> bytes:
    changed = contents
    for old, new in zip(base, variant, strict=True):
        old = native_anchor(changed, old)
        new = native_anchor(changed, new)
        if changed.count(old) != 1:
            raise MeasurementError("A reviewed Rust edit anchor is no longer unique.")
        changed = changed.replace(old, new, 1)
    return changed


def scene_variant(contents: bytes) -> bytes:
    try:
        document = json.loads(contents)
        entities = document["payload"]["entities"]
    except (UnicodeDecodeError, json.JSONDecodeError, KeyError, TypeError, RecursionError) as error:
        raise MeasurementError("The startup scene lacks its expected entity list.") from error
    if not isinstance(entities, list):
        raise MeasurementError("The startup scene entity list is not an array.")
    players = [
        entity["components"]["reference_game.Player"]
        for entity in entities
        if isinstance(entity, dict)
        and isinstance(entity.get("components"), dict)
        and "reference_game.Player" in entity["components"]
    ]
    if len(players) != 1:
        raise MeasurementError("The startup scene must contain exactly one Player component.")
    try:
        hit_points = players[0]["value"]["value"]["hit-points"]
    except (KeyError, TypeError) as error:
        raise MeasurementError("The Player component lacks its hit-points field.") from error
    if hit_points != {"type": "i64", "value": 20}:
        raise MeasurementError("The Player hit-points field is not the expected i64 value 20.")
    hit_points["value"] = 21
    text = json.dumps(document, indent=2, ensure_ascii=True) + "\n"
    if b"\r\n" in contents:
        text = text.replace("\n", "\r\n")
    return text.encode("utf-8")


def validate_terminal_summary(value: object) -> dict[str, Any]:
    fields = {
        "schema",
        "outcome",
        "tick",
        "score",
        "player_hit_points",
        "enemies_remaining",
        "projectiles_remaining",
    }
    if not isinstance(value, dict) or set(value) != fields:
        raise MeasurementError("The public headless summary has an invalid field set.")
    if value.get("schema") != "nara-reference-game.wave-summary-v1":
        raise MeasurementError("The public headless summary has an unknown schema.")
    if value.get("outcome") != "completed":
        raise MeasurementError("The public headless summary is not the completed reference wave.")
    numeric_fields = (
        "tick",
        "score",
        "player_hit_points",
        "enemies_remaining",
        "projectiles_remaining",
    )
    if any(
        not isinstance(value[field], int) or isinstance(value[field], bool)
        for field in numeric_fields
    ):
        raise MeasurementError("The public headless summary has a non-integer state field.")
    if (
        value["tick"] <= 0
        or value["score"] < 0
        or value["player_hit_points"] <= 0
        or value["enemies_remaining"] != 0
        or value["projectiles_remaining"] < 0
    ):
        raise MeasurementError("The public headless summary is not a valid completed state.")
    return value


def terminal_summary(output: bytes) -> tuple[dict[str, Any], str]:
    for line in reversed(output.splitlines()):
        try:
            value = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError, RecursionError):
            continue
        if isinstance(value, dict) and value.get("schema") == "nara-reference-game.wave-summary-v1":
            summary = validate_terminal_summary(value)
            return summary, digest(canonical_json(summary))
    raise MeasurementError("The public headless command emitted no terminal wave summary.")


def reset_directory(path: Path, owner: Path) -> None:
    path = resolve(path, "measurement target")
    owner = resolve(owner, "measurement output")
    if path == owner or not within(path, owner):
        raise MeasurementError("The measurement target escaped its output root.")
    try:
        if path.exists():
            shutil.rmtree(path)
        path.mkdir(parents=True)
    except OSError as error:
        raise MeasurementError(f"The measurement target could not be reset: {error}") from error


def remove_owned_directory(path: Path, owner: Path) -> None:
    owner = resolve(owner, "measurement output")
    if path.is_symlink():
        raise MeasurementError("A measurement scratch path became a symbolic link.")
    if not path.exists():
        return
    path = resolve(path, "measurement scratch")
    if path == owner or not within(path, owner) or not path.is_dir():
        raise MeasurementError("A measurement scratch path escaped its output root.")
    try:
        shutil.rmtree(path)
    except OSError as error:
        raise MeasurementError(f"A measurement scratch directory could not be removed: {error}") from error


class Collector:
    def __init__(
        self,
        game: Path,
        output: Path,
        cargo: str,
        environment: dict[str, str],
        timeout: float,
        revision: str,
        environment_fingerprint: str,
        requirements: dict[str, dict[str, Any]],
    ) -> None:
        self.game = game
        self.output = output
        self.cargo = cargo
        self.environment = environment
        self.timeout = timeout
        self.revision = revision
        self.environment_fingerprint = environment_fingerprint
        self.requirements = requirements
        self.records: list[dict[str, Any]] = []
        self.indices: dict[str, int] = {}

    def count(self, *metric_ids: str) -> int:
        return max(self.requirements[metric_id]["minimum_samples"] for metric_id in metric_ids)

    def command(
        self,
        arguments: Sequence[str],
        log_name: str,
    ) -> tuple[CommandResult, str]:
        reference = f"logs/{log_name}"
        result = run_process(
            [self.cargo, *arguments],
            self.game,
            self.environment,
            self.timeout,
            log_path=self.output / reference,
        )
        return result, reference

    def record(
        self,
        metric_ids: Sequence[str],
        result: CommandResult,
        value: int | None,
        log_reference: str,
        *,
        result_digest: str | None = None,
        semantic_failure: bool = False,
    ) -> None:
        failed = result.failed or semantic_failure or value is None
        for metric_id in metric_ids:
            requirement = self.requirements[metric_id]
            index = self.indices.get(metric_id, 0) + 1
            self.indices[metric_id] = index
            self.records.append(
                {
                    "metric_id": metric_id,
                    "sample_index": index,
                    "sample_value": None if failed else value,
                    "value_unit": requirement["value_kind"],
                    "population": requirement["population"],
                    "mechanism": requirement["method_id"],
                    "start_boundary": requirement["start_boundary_id"],
                    "end_boundary": requirement["end_boundary_id"],
                    "started_at_utc": result.started_at_utc,
                    "completed_at_utc": result.completed_at_utc,
                    "exit_status": result.returncode,
                    "environment_fingerprint": self.environment_fingerprint,
                    "source_revision": self.revision,
                    "command_output_reference": log_reference,
                    "result_digest": result_digest,
                }
            )


def measured_from(result: CommandResult, started_at: str, started_ns: int) -> CommandResult:
    return replace(
        result,
        started_at_utc=started_at,
        duration_ns=result.completed_monotonic_ns - started_ns,
    )


def collect_edit_population(
    collector: Collector,
    label: str,
    path: Path,
    base: bytes,
    variant: bytes,
    metric_ids: tuple[str, str],
    baseline: dict[str, Any],
    *,
    build: bool,
) -> None:
    count = collector.count(*metric_ids)
    if label == "body":
        count = max(count, collector.count("build.incremental_ns"))
    try:
        for sample in range(1, count + 1):
            use_variant = sample % 2 == 1
            write_bytes(path, variant if use_variant else base, f"{label} edit")
            started_at = utc_now()
            started_ns = time.monotonic_ns()
            if build:
                build_result, build_log = collector.command(
                    BUILD_ARGS,
                    f"{label}-build-{sample:02}.log",
                )
                if label == "body" and sample <= collector.count("build.incremental_ns"):
                    collector.record(
                        ("build.incremental_ns",),
                        build_result,
                        build_result.duration_ns,
                        build_log,
                    )
                if build_result.failed:
                    collector.record(
                        metric_ids,
                        measured_from(build_result, started_at, started_ns),
                        None,
                        build_log,
                    )
                    raise MeasurementError(f"The {label}-edit build failed.")
            headless, headless_log = collector.command(
                HEADLESS_ARGS,
                f"{label}-headless-{sample:02}.log",
            )
            measured = measured_from(headless, started_at, started_ns)
            if headless.failed:
                collector.record(
                    metric_ids,
                    measured,
                    None,
                    headless_log,
                )
                raise MeasurementError(f"The {label}-edit headless run failed.")
            try:
                summary, summary_digest = terminal_summary(headless.output)
                expected = dict(baseline)
                if label == "data" and use_variant:
                    expected["player_hit_points"] = 21
                if summary != expected:
                    raise MeasurementError(f"The {label} edit changed an unexpected game result.")
            except MeasurementError:
                collector.record(
                    metric_ids,
                    measured,
                    None,
                    headless_log,
                    semantic_failure=True,
                )
                raise
            collector.record(
                metric_ids,
                measured,
                measured.duration_ns,
                headless_log,
                result_digest=summary_digest,
            )
    finally:
        write_bytes(path, base, f"{label} edit restoration")


def command_error(result: CommandResult) -> str | None:
    if result.timed_out:
        return "timed out"
    if result.overflowed:
        return "exceeded its output budget"
    if result.returncode != 0:
        return f"exited with status {result.returncode}"
    return None


def collect_first_playable(
    subject_argument: Path,
    output_argument: Path,
    cargo: str,
    timeout: float,
) -> None:
    subject, revision = clean_subject(subject_argument)
    output = new_output(subject, output_argument)
    requirement_list, catalog_sha256 = load_requirements(subject)
    requirements = {item["id"]: item for item in requirement_list}
    try:
        output.mkdir()
        (output / "logs").mkdir()
    except OSError as error:
        raise MeasurementError(f"The measurement output could not be created: {error}") from error
    write_json(
        output / PLAN_FILENAME,
        plan_payload(revision, requirement_list, catalog_sha256),
    )

    worktree = output / "worktree"
    target = output / "target"
    environment: dict[str, str] = {}
    originals: dict[Path, bytes] = {}
    records: list[dict[str, Any]] = []
    environment_record: dict[str, Any] = {}
    checks: dict[str, Any] = {}
    baseline: dict[str, Any] | None = None
    baseline_digest: str | None = None
    collector: Collector | None = None
    worktree_added = False
    collection_error: MeasurementError | None = None
    started_at = utc_now()

    try:
        environment = isolated_environment(output, target)
        add = run_process(
            ["git", "-C", os.fspath(subject), "worktree", "add", "--detach", os.fspath(worktree), revision],
            subject,
            git_environment("1"),
            60.0,
            output_limit=MAX_GIT_OUTPUT,
            log_path=output / "logs/setup-worktree.log",
        )
        if add.failed:
            raise MeasurementError("The detached measurement worktree could not be created.")
        worktree_added = True
        game = worktree / "reference-game"
        systems = game / "src/systems.rs"
        scene = game / "scenes/startup.scene.json"
        originals = {
            systems: read_bounded(systems, MAX_SOURCE_BYTES, "systems source"),
            scene: read_bounded(scene, MAX_SOURCE_BYTES, "startup scene"),
        }
        body = rust_variant(originals[systems], (BODY_BASE,), (BODY_VARIANT,))
        structural = rust_variant(originals[systems], STRUCTURAL_BASE, STRUCTURAL_VARIANT)
        data = scene_variant(originals[scene])

        fetch = run_process(
            [cargo, "fetch", "--locked"],
            game,
            environment,
            timeout,
            log_path=output / "logs/setup-cargo-fetch.log",
        )
        if fetch.failed:
            raise MeasurementError("The isolated dependency fetch failed.")
        environment["CARGO_NET_OFFLINE"] = "true"
        cargo_version = run_process(
            [cargo, "--version"],
            game,
            environment,
            30.0,
            log_path=output / "logs/environment-cargo-version.log",
        )
        rustc_version = run_process(
            ["rustc", "--version", "--verbose"],
            game,
            environment,
            30.0,
            log_path=output / "logs/environment-rustc-version.log",
        )
        if cargo_version.failed or rustc_version.failed:
            raise MeasurementError("The isolated toolchain identity could not be inspected.")
        environment_record = {
            "os_name": platform.system(),
            "os_release": platform.release(),
            "runner_image": os.environ.get("ImageOS") or os.environ.get("RUNNER_OS") or "local",
            "rustc_version": rustc_version.output.decode(errors="replace").strip(),
            "cargo_version": cargo_version.output.decode(errors="replace").strip(),
            "cpu_model": (
                platform.processor()
                or os.environ.get("PROCESSOR_IDENTIFIER")
                or platform.machine()
            ),
            "build_profile": "debug",
            "desktop_adapter_or_software_profile": "not_collected",
            "collector_revision": digest(read_bounded(Path(__file__), MAX_SOURCE_BYTES, "collector")),
            "cargo_build_jobs": 1,
            "cargo_network": "offline_after_fetch",
        }
        fingerprint = digest(canonical_json(environment_record))
        environment_record["fingerprint"] = fingerprint
        collector = Collector(
            game,
            output,
            cargo,
            environment,
            timeout,
            revision,
            fingerprint,
            requirements,
        )

        cold_count = collector.count("build.cold_ns")
        journey_count = collector.count("journey.clean_to_headless_wave_ns")
        success_count = collector.count("gameplay.headless_wave_success")
        for sample in range(1, max(cold_count, journey_count, success_count) + 1):
            reset_directory(target, output)
            journey_started_at = utc_now()
            journey_started_ns = time.monotonic_ns()
            build_result, build_log = collector.command(
                BUILD_ARGS,
                f"cold-build-{sample:02}.log",
            )
            if sample <= cold_count:
                collector.record(
                    ("build.cold_ns",),
                    build_result,
                    build_result.duration_ns,
                    build_log,
                )
            if build_result.failed:
                raise MeasurementError(f"A cold reference-game build {command_error(build_result)}.")
            if sample > max(journey_count, success_count):
                continue
            headless, headless_log = collector.command(
                HEADLESS_ARGS,
                f"clean-headless-{sample:02}.log",
            )
            measured = measured_from(headless, journey_started_at, journey_started_ns)
            if headless.failed:
                if sample <= journey_count:
                    collector.record(
                        ("journey.clean_to_headless_wave_ns",),
                        measured,
                        None,
                        headless_log,
                    )
                if sample <= success_count:
                    collector.record(
                        ("gameplay.headless_wave_success",),
                        headless,
                        None,
                        headless_log,
                    )
                raise MeasurementError(f"A clean headless run {command_error(headless)}.")
            try:
                summary, summary_digest = terminal_summary(headless.output)
                if baseline is None:
                    baseline, baseline_digest = summary, summary_digest
                elif summary != baseline:
                    raise MeasurementError("Clean headless runs produced different terminal states.")
            except MeasurementError:
                if sample <= journey_count:
                    collector.record(
                        ("journey.clean_to_headless_wave_ns",),
                        measured,
                        None,
                        headless_log,
                        semantic_failure=True,
                    )
                if sample <= success_count:
                    collector.record(
                        ("gameplay.headless_wave_success",),
                        headless,
                        None,
                        headless_log,
                        semantic_failure=True,
                    )
                raise
            if sample <= journey_count:
                collector.record(
                    ("journey.clean_to_headless_wave_ns",),
                    measured,
                    measured.duration_ns,
                    headless_log,
                    result_digest=summary_digest,
                )
            if sample <= success_count:
                collector.record(
                    ("gameplay.headless_wave_success",),
                    headless,
                    1,
                    headless_log,
                    result_digest=summary_digest,
                )
        if baseline is None:
            raise MeasurementError("No clean headless baseline was collected.")

        collect_edit_population(
            collector,
            "body",
            systems,
            originals[systems],
            body,
            ("iteration.body.p50_ns", "iteration.body.p95_ns"),
            baseline,
            build=True,
        )
        collect_edit_population(
            collector,
            "data",
            scene,
            originals[scene],
            data,
            ("iteration.data.p50_ns", "iteration.data.p95_ns"),
            baseline,
            build=False,
        )
        collect_edit_population(
            collector,
            "structural",
            systems,
            originals[systems],
            structural,
            ("iteration.structural.p50_ns", "iteration.structural.p95_ns"),
            baseline,
            build=True,
        )
        coverage, coverage_log = collector.command(
            PUBLIC_SURFACE_ARGS,
            "public-surface-check.log",
        )
        checks["public_surface"] = {
            "exit_status": coverage.returncode,
            "log": coverage_log,
            "metric_admitted": False,
            "reason": UNAVAILABLE_REASONS["public.production.coverage_basis_points"],
        }
        if coverage.failed:
            raise MeasurementError("The public-surface check failed.")
        records = collector.records
    except MeasurementError as error:
        collection_error = error
        if collector is not None:
            records = collector.records
    finally:
        cleanup_error: MeasurementError | None = None
        for path, contents in originals.items():
            try:
                path.write_bytes(contents)
            except OSError as error:
                cleanup_error = MeasurementError(f"An isolated edit could not be restored: {error}")
        if worktree_added:
            try:
                removal = run_process(
                    ["git", "-C", os.fspath(subject), "worktree", "remove", os.fspath(worktree)],
                    subject,
                    git_environment("1"),
                    60.0,
                    output_limit=MAX_GIT_OUTPUT,
                    log_path=output / "logs/cleanup-worktree.log",
                )
                if removal.failed:
                    cleanup_error = MeasurementError("The detached worktree could not be removed.")
            except MeasurementError as error:
                cleanup_error = error
        for name in SCRATCH_DIRECTORIES:
            try:
                remove_owned_directory(output / name, output)
            except MeasurementError as error:
                cleanup_error = error
        try:
            clean_after, revision_after = clean_subject(subject)
            if clean_after != subject or revision_after != revision:
                cleanup_error = MeasurementError("The source repository changed during collection.")
        except MeasurementError as error:
            cleanup_error = error
        if cleanup_error is not None:
            collection_error = cleanup_error

        raw = b"".join(canonical_json(record) + b"\n" for record in records)
        if len(records) > MAX_RAW_SAMPLES or len(raw) > MAX_RAW_BYTES:
            collection_error = MeasurementError("The raw sample budget was exceeded.")
        write_bytes(output / RAW_FILENAME, raw, "raw sample artifact")
        observed: dict[str, int] = {}
        for record in records:
            if record["sample_value"] is not None:
                metric = record["metric_id"]
                observed[metric] = observed.get(metric, 0) + 1
        missing = build_missing_metrics(requirements, observed)
        write_json(
            output / RUN_FILENAME,
            {
                "schema": RUN_SCHEMA,
                "format_version": 2,
                "status": "collection_failed" if collection_error else "automatic_slice_complete",
                "decision": "not_evaluated",
                "source_revision": revision,
                "started_at_utc": started_at,
                "completed_at_utc": utc_now(),
                "environment": environment_record,
                "raw_sample_count": len(records),
                "raw_samples_sha256": digest(raw),
                "base_terminal_summary": baseline,
                "base_terminal_summary_digest": baseline_digest,
                "observed_sample_counts": observed,
                "missing_metrics": missing,
                "checks": checks,
                "failure": str(collection_error) if collection_error else None,
                "non_claims": list(RUN_NON_CLAIMS),
            },
        )
    if collection_error is not None:
        raise collection_error


def missing_reason(metric: str) -> str:
    if metric == "gameplay.desktop_playable_success":
        return "Manual desktop playability was not inferred from process exit."
    if metric == "journey.clean_to_desktop_playable_ns":
        return "No current human-timed clean desktop journey was supplied."
    return UNAVAILABLE_REASONS.get(
        metric,
        "The automatic collection did not reach the metric's required sample count.",
    )


def build_missing_metrics(
    requirements: dict[str, dict[str, Any]], observed: dict[str, int]
) -> list[dict[str, Any]]:
    return [
        {
            "id": metric,
            "required_samples": requirement["minimum_samples"],
            "observed_samples": observed.get(metric, 0),
            "reason": missing_reason(metric),
        }
        for metric, requirement in sorted(requirements.items())
        if observed.get(metric, 0) < requirement["minimum_samples"]
    ]


def verify_log(run: Path, reference: object) -> str:
    if not isinstance(reference, str) or not reference.startswith("logs/"):
        raise MeasurementError("A raw sample has an invalid log reference.")
    relative = Path(reference)
    if relative.is_absolute() or ".." in relative.parts or relative.as_posix() != reference:
        raise MeasurementError("A raw sample log reference escaped the run.")
    path = resolve(run / relative, "sample log")
    if not within(path, run) or not path.is_file() or path.is_symlink():
        raise MeasurementError("A raw sample log is not a regular run-owned file.")
    read_bounded(path, MAX_COMMAND_OUTPUT, "sample log")
    return reference


def is_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def collector_repository() -> Path:
    collector = resolve(Path(__file__), "collector")
    try:
        repository = collector.parents[2]
    except IndexError as error:
        raise MeasurementError("The collector is not inside its repository layout.") from error
    if not (repository / CATALOG_PATH).is_file():
        raise MeasurementError("The collector cannot locate its committed metric catalog.")
    return repository


def verify_environment(environment: object, *, complete: bool, has_records: bool) -> str | None:
    if not isinstance(environment, dict):
        raise MeasurementError("The run environment is not an object.")
    if not environment:
        if complete or has_records:
            raise MeasurementError("A completed or sampled run lacks its environment.")
        return None
    expected_fields = {
        *ENVIRONMENT_FIELDS,
        "cargo_build_jobs",
        "cargo_network",
        "fingerprint",
    }
    if set(environment) != expected_fields:
        raise MeasurementError("The run environment has an invalid field set.")
    if any(
        not isinstance(environment[field], str) or not environment[field].strip()
        for field in ENVIRONMENT_FIELDS
    ):
        raise MeasurementError("The run environment has an empty identity field.")
    if environment["cargo_build_jobs"] != 1 or environment["cargo_network"] != "offline_after_fetch":
        raise MeasurementError("The run environment changed its Cargo isolation contract.")
    collector_sha256 = digest(read_bounded(Path(__file__), MAX_SOURCE_BYTES, "collector"))
    if environment["collector_revision"] != collector_sha256:
        raise MeasurementError("The run was produced by a different collector revision.")
    fingerprint = environment["fingerprint"]
    fingerprint_input = dict(environment)
    fingerprint_input.pop("fingerprint")
    if not is_sha256(fingerprint) or fingerprint != digest(canonical_json(fingerprint_input)):
        raise MeasurementError("The run environment fingerprint does not match.")
    return fingerprint


def verify_run(argument: Path) -> None:
    run = resolve(argument, "measurement run")
    if not run.is_dir():
        raise MeasurementError("The measurement run must be a directory.")
    manifest = read_json(run / RUN_FILENAME, MAX_MANIFEST_BYTES, "run manifest")
    plan = read_json(run / PLAN_FILENAME, MAX_MANIFEST_BYTES, "measurement plan")
    raw = read_bounded(run / RAW_FILENAME, MAX_RAW_BYTES, "raw samples")
    status = manifest.get("status")
    if (
        set(manifest)
        != {
            "schema",
            "format_version",
            "status",
            "decision",
            "source_revision",
            "started_at_utc",
            "completed_at_utc",
            "environment",
            "raw_sample_count",
            "raw_samples_sha256",
            "base_terminal_summary",
            "base_terminal_summary_digest",
            "observed_sample_counts",
            "missing_metrics",
            "checks",
            "failure",
            "non_claims",
        }
        or manifest.get("schema") != RUN_SCHEMA
        or manifest.get("format_version") != 2
        or status not in {"automatic_slice_complete", "collection_failed"}
        or manifest.get("decision") != "not_evaluated"
    ):
        raise MeasurementError("The run manifest has an invalid contract.")
    failure = manifest.get("failure")
    if (status == "collection_failed") != (isinstance(failure, str) and bool(failure)):
        raise MeasurementError("The run status and failure do not agree.")
    revision = manifest.get("source_revision")
    if (
        not isinstance(revision, str)
        or len(revision) != 40
        or any(character not in "0123456789abcdef" for character in revision)
    ):
        raise MeasurementError("The run lacks one full source revision.")
    expected_requirements, catalog_sha256 = load_requirements(collector_repository())
    if plan != plan_payload(revision, expected_requirements, catalog_sha256):
        raise MeasurementError("The run plan does not match the committed protocol.")
    requirements = {item["id"]: item for item in expected_requirements}
    if manifest.get("raw_samples_sha256") != digest(raw):
        raise MeasurementError("The manifest does not bind the raw samples.")
    lines = raw.splitlines()
    if len(lines) > MAX_RAW_SAMPLES or manifest.get("raw_sample_count") != len(lines):
        raise MeasurementError("The manifest does not bind the raw sample count.")
    fingerprint = verify_environment(
        manifest.get("environment"),
        complete=status == "automatic_slice_complete",
        has_records=bool(lines),
    )

    observed: dict[str, int] = {}
    indices: dict[str, int] = {}
    series: dict[str, list[tuple[object, object, object]]] = {}
    verified_logs: set[str] = set()
    for line in lines:
        if not line or len(line) > MAX_RAW_LINE_BYTES:
            raise MeasurementError("A raw sample is empty or oversized.")
        try:
            record = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
            raise MeasurementError("A raw sample is invalid JSON.") from error
        if not isinstance(record, dict) or set(record) != RAW_FIELD_SET:
            raise MeasurementError("A raw sample has an invalid field set.")
        metric = record["metric_id"]
        if metric not in requirements:
            raise MeasurementError("A raw sample names an unknown metric.")
        expected_index = indices.get(metric, 0) + 1
        if (
            not isinstance(record["sample_index"], int)
            or isinstance(record["sample_index"], bool)
            or record["sample_index"] != expected_index
        ):
            raise MeasurementError("Raw sample indices are not contiguous.")
        indices[metric] = expected_index
        value = record["sample_value"]
        exit_status = record["exit_status"]
        if (
            not isinstance(exit_status, int)
            or isinstance(exit_status, bool)
            or (
                value is not None
                and (
                    not isinstance(value, int)
                    or isinstance(value, bool)
                    or value < 0
                )
            )
        ):
            raise MeasurementError("A raw sample has an invalid numeric field.")
        if requirements[metric]["value_kind"] == "boolean" and value not in {None, 0, 1}:
            raise MeasurementError("A boolean raw sample is not zero or one.")
        if metric == "gameplay.headless_wave_success" and value not in {None, 1}:
            raise MeasurementError("The headless success metric contains an explicit false value.")
        for field in (
            "started_at_utc",
            "completed_at_utc",
        ):
            if not isinstance(record[field], str) or not record[field]:
                raise MeasurementError(f"A raw sample has an invalid `{field}` field.")
        requirement = requirements[metric]
        if (
            record["value_unit"] != requirement["value_kind"]
            or record["population"] != requirement["population"]
            or record["mechanism"] != requirement["method_id"]
            or record["start_boundary"] != requirement["start_boundary_id"]
            or record["end_boundary"] != requirement["end_boundary_id"]
        ):
            raise MeasurementError("A raw sample does not match its metric protocol.")
        if record["source_revision"] != revision or record["environment_fingerprint"] != fingerprint:
            raise MeasurementError("A raw sample does not belong to this run.")
        log = record["command_output_reference"]
        if not isinstance(log, str):
            raise MeasurementError("A raw sample has an invalid log reference.")
        if log not in verified_logs:
            verify_log(run, log)
            verified_logs.add(log)
        failed = value is None or exit_status != 0
        result_digest = record["result_digest"]
        if result_digest is not None and not is_sha256(result_digest):
            raise MeasurementError("A raw sample result digest is invalid.")
        if not failed and metric in SEMANTIC_METRICS and not is_sha256(result_digest):
            raise MeasurementError("A semantic raw sample lacks its result digest.")
        if not failed:
            observed[metric] = observed.get(metric, 0) + 1
        series.setdefault(metric, []).append((value, exit_status, result_digest))
    if manifest.get("observed_sample_counts") != observed:
        raise MeasurementError("The manifest sample counts do not match the raw samples.")
    for left, right in TWIN_METRICS:
        if series.get(left) != series.get(right):
            raise MeasurementError("A P50/P95 pair does not share one raw population.")
    if status == "automatic_slice_complete" and any(
        observed.get(metric, 0) < requirements[metric]["minimum_samples"]
        for metric in AUTOMATIC_METRICS
    ):
        raise MeasurementError("The completed automatic slice lacks required samples.")
    missing = build_missing_metrics(requirements, observed)
    if manifest.get("missing_metrics") != missing:
        raise MeasurementError("The manifest does not truthfully name missing metrics.")
    if manifest.get("non_claims") != list(RUN_NON_CLAIMS):
        raise MeasurementError("The run changed its non-claims.")
    summary = manifest.get("base_terminal_summary")
    if summary is not None:
        _, summary_digest = terminal_summary(canonical_json(summary) + b"\n")
        if manifest.get("base_terminal_summary_digest") != summary_digest:
            raise MeasurementError("The terminal summary digest does not match.")
        data_variant = dict(summary)
        data_variant["player_hit_points"] = 21
        data_variant_digest = digest(canonical_json(data_variant))
        for metric in (
            "gameplay.headless_wave_success",
            "journey.clean_to_headless_wave_ns",
        ):
            if any(
                value is not None and exit_status == 0 and result != summary_digest
                for value, exit_status, result in series.get(metric, ())
            ):
                raise MeasurementError("A base-wave sample changed its terminal result.")
        for metric, records in series.items():
            if metric.startswith("iteration.body.") or metric.startswith(
                "iteration.structural."
            ):
                if any(
                    value is not None and exit_status == 0 and result != summary_digest
                    for value, exit_status, result in records
                ):
                    raise MeasurementError("A behavior-neutral edit changed its terminal result.")
            elif metric.startswith("iteration.data."):
                for index, (value, exit_status, result) in enumerate(records, start=1):
                    expected = summary_digest if index % 2 == 0 else data_variant_digest
                    if value is not None and exit_status == 0 and result != expected:
                        raise MeasurementError("A data edit result does not match its variant.")
    elif status == "automatic_slice_complete":
        raise MeasurementError("A completed run lacks its terminal summary.")
    checks = manifest.get("checks")
    if not isinstance(checks, dict) or not set(checks).issubset({"public_surface"}):
        raise MeasurementError("The run checks are not an object.")
    public_surface = checks.get("public_surface")
    if public_surface is not None:
        expected_fields = {"exit_status", "log", "metric_admitted", "reason"}
        if (
            not isinstance(public_surface, dict)
            or set(public_surface) != expected_fields
            or not isinstance(public_surface.get("exit_status"), int)
            or isinstance(public_surface.get("exit_status"), bool)
            or public_surface.get("log") != "logs/public-surface-check.log"
            or public_surface.get("metric_admitted") is not False
            or public_surface.get("reason")
            != UNAVAILABLE_REASONS["public.production.coverage_basis_points"]
        ):
            raise MeasurementError("The public-surface check has an invalid contract.")
        verify_log(run, public_surface["log"])
    if status == "automatic_slice_complete":
        if public_surface is None or public_surface["exit_status"] != 0:
            raise MeasurementError("The completed run lacks its public-surface check.")
    elif failure == "The public-surface check failed.":
        if public_surface is None or public_surface["exit_status"] == 0:
            raise MeasurementError("The failed public-surface check lost its evidence.")
    elif public_surface is not None and public_surface["exit_status"] != 0:
        raise MeasurementError("The run failure does not match its public-surface check.")


def positive_seconds(value: str) -> float:
    try:
        result = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be a number") from error
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("must be finite and greater than zero")
    return result


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description=(
            "Collect or verify one isolated reference-game baseline; this helper "
            "does not evaluate a performance or product result."
        )
    )
    commands = result.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", help="write a non-executing collection plan")
    plan.add_argument("--subject", type=Path, required=True)
    plan.add_argument("--output", type=Path, required=True)
    collect = commands.add_parser("collect", help="collect the automatic slice")
    collect.add_argument("--subject", type=Path, required=True)
    collect.add_argument("--output", type=Path, required=True)
    collect.add_argument("--cargo", default="cargo")
    collect.add_argument(
        "--command-timeout-seconds",
        type=positive_seconds,
        default=DEFAULT_COMMAND_TIMEOUT_SECONDS,
    )
    verify = commands.add_parser("verify", help="verify one fresh local run")
    verify.add_argument("--run", type=Path, required=True)
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "plan":
            create_plan(options.subject, options.output)
        elif options.command == "collect":
            collect_first_playable(
                options.subject,
                options.output,
                options.cargo,
                options.command_timeout_seconds,
            )
        elif options.command == "verify":
            verify_run(options.run)
        else:
            raise AssertionError(f"unhandled command: {options.command}")
    except MeasurementError as error:
        print(f"measurement-helper: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
